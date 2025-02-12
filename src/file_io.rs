use std::collections::HashMap;
use std::fmt;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};

//use crate::{audio::FreqData, audio::ShortTimeDftData, audio::WindowFunction, util::get_arr_from_slice};
use crate::util::get_arr_from_slice;

#[derive(Clone)]
pub struct WavInfo {
    pub sample_type: u8,
    pub channels: u8,
    pub sample_rate: u32,
    pub data_rate: u32,
    pub data_block_size: u32,
    pub bit_depth: u32,
    pub byte_depth: u32,
    pub chunks: HashMap<String, (u64, u32)>, // {chunk_name: (position, chunk_size)}
    pub file_size: u32,
    pub audio_duration: f32,
}

impl WavInfo {
    pub fn new(
        sample_type: u8,
        channels: u8,
        sample_rate: u32,
        bit_depth: u32,
        file_size: u32,
        chunks: HashMap<String, (u64, u32)>,
    ) -> Self {
        let byte_depth = bit_depth / 8;
        let audio_duration = chunks.get("data").unwrap().1 as f32
            / (byte_depth * channels as u32 * sample_rate) as f32;
        WavInfo {
            sample_type,
            channels,
            sample_rate,
            data_rate: sample_rate * byte_depth * channels as u32,
            data_block_size: byte_depth * channels as u32,
            bit_depth,
            byte_depth: bit_depth / 8,
            chunks,
            file_size,
            audio_duration,
        }
    }
}

impl fmt::Display for WavInfo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sample_type = match self.sample_type {
            1 => "PCM",
            3 => "IEEE Float",
            6 => "8-bit ITU-T G.711 A-law",
            7 => "8-bit ITU-T G.711 µ-law",
            254 => "Wav Extensible Format",
            _ => "Unsupported",
        };
        f.write_str(format!("Wav Info:\nSample Type: {}\nSample rate: {} Hz\nSample size: {} bit\nBlock Size: {} bytes\nData Rate: {} bytes/sec\nChannels: {}\nDuration: {} secs\nFile size: {} bytes", sample_type, self.sample_rate, self.bit_depth, self.data_block_size, self.data_rate, self.channels, self.audio_duration, self.file_size).as_str())
    }
}

pub fn read_wav_meta(f: &mut BufReader<File>) -> WavInfo {
    f.seek(SeekFrom::Start(0)).unwrap();

    f.seek_relative(4).unwrap();
    let f_size: u32 = read_le_uint(f, 4);
    f.seek_relative(12).unwrap();
    let mut fmt_code = read_le_uint(f, 2) as u8;
    let channels = read_le_uint(f, 2) as u8;
    let sample_rate = read_le_uint(f, 4);
    let _data_rate = read_le_uint(f, 4); // not used since WavInfo::new calculates these
    let _data_block_size = read_le_uint(f, 2); // not used since WavInfo::new calculates these
    let bit_depth = read_le_uint(f, 2);

    //only really reading this stuff for potential future use, its not used at the moment
    let ext_size: u8;
    let _v_bits_per_sample: u8; // information about the precision of IEEE floats in file, unused here
    let _channel_mask: u32; // mapping from channels to physical speakers, isnt used here
    let _subformat: String;

    // bit depths of 8 or less in PCM use offset binary instead of 
    // 2's complement which idk how to parse so ..
    if fmt_code == 1 && bit_depth <= 8 {
        panic!("Unsupported bit depth (bit depth 8 or lower)");
    }

    match fmt_code {
        1 => {}, //no extra parsing needed for PCM data
        3 | 6 | 7 | 0xFE => { //non-PCM data should always have the ext_size field
            ext_size = read_le_uint(f, 2) as u8;
            if ext_size > 0 {
                _v_bits_per_sample = read_le_uint(f, 2) as u8;
                _channel_mask = read_le_uint(f, 4);
                // files with extension data store the actual format code
                // late in the file so now we read it in again ...
                fmt_code = read_le_uint(f, 2) as u8;
            }
        },
        _ => {
            panic!("Unknown format code!");
        }
    }

    f.seek(SeekFrom::Start(0)).unwrap();
    f.seek_relative(12).unwrap(); //skip 'RIFF' and 'WAVE' tags
    let mut chunks: HashMap<String, (u64, u32)> = HashMap::new();

    while f.stream_position().unwrap() < f_size as u64 {
        let title = read_str(f, 4);
        let size = read_le_uint(f, 4);
        chunks.insert(title, (f.stream_position().unwrap(), size));
        f.seek_relative(size as i64).unwrap();
    }

    f.seek(SeekFrom::Start(0)).unwrap();
    WavInfo::new(fmt_code, channels, sample_rate, bit_depth, f_size, chunks)
}

pub fn read_data(
    f: &mut BufReader<File>,
    file_info: WavInfo,
    start_time: f32,
    duration: f32,
) -> Option<Vec<Vec<f32>>> {
    let sample_size = (file_info.bit_depth / 8) as usize;
    let channels = file_info.channels as usize;
    let mut samples_per_channel = (duration * file_info.sample_rate as f32) as usize;
    let total_samples = samples_per_channel * channels;

    f.seek(SeekFrom::Start(
        file_info.chunks.get("data".into()).unwrap().0,
    ))
    .unwrap();
    let file_start_pos = (start_time * file_info.sample_rate as f32 * file_info.channels as f32) as i64;
    //skip to start_pos in the file
    f.seek_relative(file_start_pos)
        .unwrap();

    let mut data: Vec<u8>;
    let (data_start, data_size) = *file_info.chunks.get("data").unwrap();
    //either read the amount of data requested, or read to EOF
    if f.stream_position().unwrap() + total_samples as u64 * sample_size as u64 > (data_start + data_size as u64) {
        data = vec![0; (data_start + data_size as u64) as usize - f.stream_position().unwrap() as usize];
        f.read_exact(&mut data).unwrap();
        samples_per_channel = data.len() / channels / sample_size;
    } else {
        data = vec![0; total_samples * sample_size];
        f.read_exact(&mut data).unwrap();
    }

    let mut output = vec![vec![0.; samples_per_channel]; channels];

    match file_info.sample_type {
        1 => { //Regular PCM data
            match file_info.bit_depth {
                16 => {
                    for i in 0..samples_per_channel {
                        let idx = i * sample_size * channels;
                        for j in 0..channels {
                            let ch_offset = j * sample_size + idx;
                            output[j][i] = (((data[ch_offset + 1] as i32) << 24
                                | (data[ch_offset] as i32) << 16)
                                >> 16) as f32
                                / 0xFFFF as f32;
                        }
                    }
                }

                24 => {
                    for i in 0..samples_per_channel {
                        let idx = i * sample_size * channels;
                        for j in 0..channels {
                            let ch_idx = j * sample_size + idx;
                            output[j][i] = (((data[ch_idx + 2] as i32) << 24
                                | (data[ch_idx + 1] as i32) << 16
                                | (data[ch_idx] as i32) << 8)
                                >> 8) as f32
                                / 0xFFFFFF as f32;
                        }
                    }
                }

                32 => {
                    for i in 0..samples_per_channel {
                        let idx = i * sample_size * channels;
                        for j in 0..channels {
                            let ch_offset = j * sample_size + idx;
                            output[j][j] = (((data[ch_offset + 3] as i32) << 24
                                | (data[ch_offset + 2] as i32) << 16
                                | (data[ch_offset + 1] as i32) << 8)
                                | (data[ch_offset] as i32)) as f32
                                / (i32::MAX) as f32;
                        }
                    }
                }

                _ => return None,
            }
        },
        3 => { // IEEE float data
            // Wav supports 64-bit float so may implement this in future but it is very uncommon
            if file_info.bit_depth > 32 {
                panic!("Unsupported bit depth!");
            }
            for i in 0..samples_per_channel {
                let idx = i * sample_size * channels;
                for j in 0..channels {
                    let ch_idx = j * sample_size + idx;
                    let dat: [u8; 4] = get_arr_from_slice(&data[ch_idx..ch_idx+sample_size]);
                    output[j][i] = f32::from_le_bytes(dat);
                }
            }
        }
        _ => {
            panic!("Unsupported file format!");
        },
    }


    Some(output)
}
//
//pub fn write_stdft_to_file(file_dir: String, stdft: &ShortTimeDftData) {
//    let mut file = BufWriter::new(File::create(file_dir.trim()).unwrap());
//
//    file.write_fmt(format_args!("{}\n", stdft.window_type.to_string()))
//        .unwrap();
//    file.write_all(&stdft.overlap.to_be_bytes()).unwrap();
//    file.write_all(&stdft.num_dfts.to_le_bytes()).unwrap();
//    file.write_all(&stdft.num_freq.to_le_bytes()).unwrap();
//    file.write_all(&stdft.sample_rate.to_le_bytes()).unwrap();
//    //cast to u32 to ensure data_size is written as 4 bytes, since usize can change sizes
//    file.write_all(&(stdft.data_size as u32).to_le_bytes())
//        .unwrap();
//
//    for j in 0..stdft.num_dfts as usize {
//        for k in 0..stdft.num_freq as usize {
//            file.write_all(&stdft.dft_data[j][k].frequency.to_be_bytes())
//                .unwrap();
//            file.write_all(&stdft.dft_data[j][k].amplitude.to_be_bytes())
//                .unwrap();
//            file.write_all(&stdft.dft_data[j][k].phase.to_be_bytes())
//                .unwrap();
//        }
//    }
//    file.flush().unwrap();
//}
//
//pub fn read_stdft_from_file(file_dir: &str) -> ShortTimeDftData {
//    let mut file = BufReader::new(File::open(file_dir).unwrap());
//
//    let mut window_str = String::new();
//    file.read_line(&mut window_str).unwrap();
//    let window_type = WindowFunction::from_str(window_str.trim()).unwrap();
//    let mut overlap_bytes = [0; 4];
//    file.read_exact(&mut overlap_bytes).unwrap();
//
//    let overlap = f32::from_be_bytes(overlap_bytes);
//    let num_dfts = read_le_uint(&mut file, 4);
//    let num_freq = read_le_uint(&mut file, 4);
//    let sample_rate = read_le_uint(&mut file, 4);
//    let data_size = read_le_uint(&mut file, 4);
//
//    let mut dft_data = vec![vec![FreqData::ZERO; num_freq as usize]; num_dfts as usize];
//    for i in 0..num_dfts as usize {
//        let mut cur_dft_dat = vec![0 as u8; num_freq as usize * 4 * 3];
//        file.read_exact(&mut cur_dft_dat).unwrap();
//        for (k, bytes) in cur_dft_dat.chunks(12).enumerate() {
//            dft_data[i][k].frequency = f32::from_be_bytes(bytes[0..4].try_into().unwrap());
//            dft_data[i][k].amplitude = f32::from_be_bytes(bytes[4..8].try_into().unwrap());
//            dft_data[i][k].phase = f32::from_be_bytes(bytes[8..12].try_into().unwrap());
//        }
//    }
//
//    ShortTimeDftData::new_with_size(
//        dft_data,
//        window_type,
//        overlap,
//        num_dfts,
//        num_freq,
//        sample_rate,
//        data_size as usize,
//    )
//}

pub fn read_data_interleaved_unchecked<T: Read>(
    f: &mut BufReader<T>,
    file_info: &WavInfo,
    data_len: usize,
) -> Vec<f32> {
    let byte_depth = file_info.byte_depth as usize;
    let mut data = vec![0; data_len * byte_depth];

    match f.read_exact(&mut data) {
        Ok(_) => (),
        Err(err) => {
            panic!("Unexpected error while reading file: {}", err)
        }
    };
    let mut out_data = vec![0.; data_len];

    match byte_depth {
        2 => {
            for j in 0..out_data.len() {
                let idx = j * byte_depth;
                out_data[j] = (((data[idx + 1] as i32) << 24 | (data[idx] as i32) << 16) >> 16)
                    as f32
                    / 0xFFFF as f32;
            }
        }

        3 => {
            for j in 0..out_data.len() {
                let idx = j * byte_depth;
                out_data[j] = (((data[idx + 2] as i32) << 24
                    | (data[idx + 1] as i32) << 16
                    | (data[idx] as i32) << 8)
                    >> 8) as f32
                    / 0xFFFFFF as f32;
            }
        }

        4 => {
            for j in 0..out_data.len() {
                let idx = j * byte_depth;
                out_data[j] = (((data[idx + 3] as i32) << 24
                    | (data[idx + 2] as i32) << 16
                    | (data[idx + 1] as i32) << 8)
                    | (data[idx] as i32)) as f32
                    / (i32::MAX) as f32;
            }
        }

        _ => panic!("Unsupported byte depth"),
    }

    out_data
}

pub fn read_str(f: &mut BufReader<File>, bytes: usize) -> String {
    let mut buf = vec![0; bytes];
    let _ = f.read_exact(&mut buf);
    buf.iter().map(|&e| e as char).collect::<String>()
}

pub fn read_le_uint(f: &mut BufReader<File>, bytes: usize) -> u32 {
    if bytes > 4 {
        return 0;
    }
    let mut buf = vec![0 as u8; bytes];
    let _ = f.read_exact(&mut buf);
    buf_to_int(&mut buf, bytes)
}

pub fn buf_to_int(buf: &[u8], bytes: usize) -> u32 {
    let mut out: u32 = 0;
    for i in (1..bytes + 1).rev() {
        out |= (buf[i - 1] as u32) << ((i - 1) * 8);
    }
    out
}
