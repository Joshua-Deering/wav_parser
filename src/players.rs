use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Data, Host, OutputCallbackInfo, SampleFormat, SampleRate, Stream};
use std::fs::File;
use std::io::{BufReader, Seek, SeekFrom};
use std::sync::{Mutex, Arc};

use crate::file_io::{read_data_interleaved_unchecked, read_wav_meta, WavInfo};
//use crate::audio::{WindowFunction, ShortTimeDftData, do_short_time_fourier_transform};

pub struct AudioPlayer {
    internal_player: Arc<Mutex<FilePlayer>>,
    pub playing: bool,
    stream: Stream,
}

impl AudioPlayer {
    pub fn new(file_path: String) -> Self {
        let mut reader = BufReader::new(File::open(format!("./res/audio/{}", file_path)).unwrap());
        let meta = read_wav_meta(&mut reader);
        
        let internal_player = Arc::new(Mutex::new(FilePlayer::new_from_reader(reader, meta.clone())));
        internal_player.lock().unwrap().paused = true;

        let sample_rate = meta.sample_rate;
        
        let host: Host = cpal::default_host();
        let device = host.default_output_device().expect("No audio device available!");

        let mut supported_output_configs = device.supported_output_configs().expect("Error querying output configs!");
        let config = supported_output_configs
            .find(|&e| e.max_sample_rate() == SampleRate(sample_rate))
            .expect("No supported output configs!")
            .with_sample_rate(SampleRate(sample_rate))
            .config();

        let stream_player_copy = Arc::clone(&internal_player);
        let stream = device
            .build_output_stream_raw(
                &config,
                SampleFormat::F32,
                move |data: &mut Data, _: &OutputCallbackInfo| {
                    stream_player_copy.lock().unwrap().next_chunk(data);
                },
                move |_err| {},
                None
            ).unwrap();
        stream.pause().unwrap();

        Self {
            internal_player,
            playing: false,
            stream,
        }
    }

    pub fn start(&mut self) {
        self.internal_player.lock().unwrap().paused = false;
        self.stream.play().unwrap();
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.internal_player.lock().unwrap().paused = true;
        self.stream.pause().unwrap();
        self.playing = false;
    }

    pub fn set_progress(&mut self, prog: f32) {
        self.internal_player.lock().unwrap().set_progress(prog);
    }

    pub fn get_progress(&self) -> f32 {
        self.internal_player.lock().unwrap().progress
    }
}

pub struct SignalPlayer {
    pub samples: Vec<Vec<f32>>,
    pub sample_rate: u32,
    channels: usize,
    //duration: f32,
    pos: usize,
    pub finished: bool,
}

impl SignalPlayer {
    pub fn new(samples: Vec<Vec<f32>>, sample_rate: u32, channels: usize) -> Self {
        //let duration = samples.len() as f32 / sample_rate as f32;
        Self {
            samples,
            sample_rate,
            channels,
            //duration,
            pos: 0,
            finished: false,
        }
    }

    //pub fn do_short_time_fourier_transform(
    //    &self,
    //    window_size: f32,
    //    overlap: f32,
    //    window_func: WindowFunction,
    //    channel: usize,
    //) -> ShortTimeDftData {
    //    let dft_data = do_short_time_fourier_transform(
    //        &self.samples[channel],
    //        self.sample_rate,
    //        window_size,
    //        overlap,
    //        window_func,
    //    );
    //
    //    let dfts = dft_data.len() as u32;
    //    let freqs = dft_data[0].len() as u32;
    //    ShortTimeDftData::new(
    //        dft_data,
    //        window_func,
    //        overlap,
    //        dfts,
    //        freqs,
    //        self.sample_rate,
    //    )
    //}
}

impl Play for SignalPlayer {
    fn next_chunk(&mut self, data: &mut Data) {
        let dat_slice = data.as_slice_mut().unwrap();
        let end = self.pos + (dat_slice.len() / self.channels);
        if end >= self.samples[0].len() {
            self.finished = true;
            return;
        }
        for i in self.pos..end {
            for c in 0..self.channels {
                dat_slice[(i - self.pos) * self.channels + c] = self.samples[c][i];
            }
        }
        self.pos += data.len() / self.channels;
    }
}

pub struct FilePlayer {
    pub file_meta: WavInfo,
    pub finished: bool,
    pub paused: bool,
    pub progress: f32,
    reader: BufReader<File>,
    pos: usize,
    start_pos: usize,
    end_pos: usize,
    size: usize,
}

impl FilePlayer {
    pub fn new(file_path: String) -> Self {
        let mut reader = BufReader::new(File::open(format!("./res/audio/{}", file_path)).unwrap());
        let file_meta = read_wav_meta(&mut reader);
        //advance reader to beginning of audio data
        reader
            .seek(SeekFrom::Start(file_meta.chunks.get("data").unwrap().0))
            .unwrap();
        let start_pos = reader.stream_position().unwrap() as usize;
        let end_pos = (file_meta.file_size / (file_meta.bit_depth / 8)) as usize;

        Self {
            file_meta,
            finished: false,
            paused: false,
            progress: 0.,
            reader,
            pos: 0,
            start_pos,
            end_pos,
            size: end_pos - start_pos,
        }
    }

    pub fn new_from_reader(mut reader: BufReader<File>, file_meta: WavInfo) -> Self {
        //advance reader to beginning of audio data
        reader
            .seek(SeekFrom::Start(file_meta.chunks.get("data").unwrap().0))
            .unwrap();
        let start_pos = reader.stream_position().unwrap() as usize;
        let end_pos = start_pos + file_meta.chunks.get("data").unwrap().1 as usize;
        Self {
            file_meta,
            finished: false,
            paused: false,
            progress: 0.,
            reader,
            pos: 0,
            start_pos,
            end_pos,
            size: end_pos - start_pos,
        }
    }

    pub fn set_progress(&mut self, prog: f32) {
        let mut new_pos = (prog * self.size as f32) as usize; 
        //this pos must be a multiple of the bit depth and channels
        new_pos -= new_pos % self.file_meta.data_block_size as usize;

        new_pos = new_pos.clamp(self.start_pos, self.end_pos);

        self.pos = new_pos;
        self.reader.seek(SeekFrom::Start(self.pos as u64)).unwrap();
    }
}

impl Play for FilePlayer {
    fn next_chunk(&mut self, data: &mut Data) {
        if self.paused {
            return;
        }

        let dat_slice = data.as_slice_mut().unwrap();
        if self.pos + dat_slice.len() * self.file_meta.byte_depth as usize >= self.end_pos {
            self.finished = true;
            return;
        }

        let data =
            read_data_interleaved_unchecked(&mut self.reader, &self.file_meta, dat_slice.len());
        dat_slice[..].clone_from_slice(&data);

        self.pos += data.len() * self.file_meta.byte_depth as usize;
        self.progress = self.pos as f32 / (self.size) as f32;
    }
}

pub trait Play {
    fn next_chunk(&mut self, data: &mut Data);
}
