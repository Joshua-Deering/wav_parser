mod audio;
mod fft;
mod file_io;
mod img_generator;
mod players;
mod util;
mod parametric_eq;
//
//use audio::WindowFunction;
//use file_io::{read_data, read_stdft_from_file, read_wav_meta, write_stdft_to_file, WavInfo};
//use img_generator::{generate_spectrogram_img, generate_waveform_img};
//use players::{FilePlayer, Play, SignalPlayer};

//use core::f32;
//use std::fs::File;
//use std::io::{stdin, BufReader};
//use std::sync::{Arc, Mutex};
//use std::time::Duration;
//use std::{thread, thread::sleep};

use util::*;
use file_io::read_wav_meta;
use img_generator::{generate_eq_response, generate_waveform};
use players::AudioPlayer;
use parametric_eq::ParametricEq;

use slint::{run_event_loop, Image, Model, ModelRc, SharedString, Timer, TimerMode, VecModel};
use cpal::{traits::{DeviceTrait, HostTrait}, SampleRate, SupportedOutputConfigs, SupportedStreamConfigRange};

use std::{fs::File, ops::Deref};
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;
use std::sync::{Mutex, Arc};

slint::include_modules!();

//standard initial 2-stage weighting curve for LKFS measurement
//param_eq.add_biquad(Biquad::with_coefficients(1.53512485958697, -2.69169618940638, 1.19839281085285, -1.69065929318241, 0.73248077421585, 48000));
//param_eq.add_biquad(Biquad::with_coefficients(1., -2., 1., -1.99004745483398, 0.99007225036621, 48000));

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let main_window = MainWindow::new()?;

    main_window.on_render_waveform(generate_waveform);

    let player: Rc<RefCell<Option<AudioPlayer>>> = Rc::new(RefCell::new(None));
    let player_eq = Arc::new(Mutex::new(ParametricEq::new(vec![], 48000)));


    // UI Initialization Logic (called when any menu is opened) ----------------
    let init_ptr = main_window.as_weak();
    main_window.on_init_menu(move |menu: i32| {
        let main_window = init_ptr.upgrade().unwrap();

        match menu {
            0 => {
                let files: Vec<SharedString> = query_directory("./res/audio/")
                    .into_iter()
                    .map(|e| SharedString::from(e))
                    .collect();

                let host = cpal::default_host();
                let device = host.default_output_device().unwrap();
                let supported_configs: Vec<_> = device.supported_output_configs().unwrap().collect();

                let mut supported_files: Vec<SharedString> = vec![];
                for f in files {
                    let mut reader = BufReader::new(File::open(format!("./res/audio/{}", f)).unwrap());
                    let f_info = read_wav_meta(&mut reader);

                    if let Some(_) = supported_configs.iter().find(|e| {
                            e.max_sample_rate() == SampleRate(f_info.sample_rate)
                    }) {
                        supported_files.push(f);
                    }
                }

                let model_rc = Rc::new(VecModel::from(supported_files));
                main_window.set_audio_files(ModelRc::from(model_rc.clone()));

                main_window.set_selected_file("".into());
            },
            _ => {}
        }
    });

    // UI Closure logic (called when any menu is closed) -----------------------
    let close_menu_player_ptr = Rc::clone(&player);
    let close_menu_window_ptr = main_window.as_weak();
    main_window.on_close_menu(move | menu: i32 | {
        let main_window = close_menu_window_ptr.upgrade().unwrap();
        match menu {
            0 => { 
                *close_menu_player_ptr.borrow_mut() = None;
                main_window.set_slider_pos(0.);
                main_window.set_is_playing(false);
                main_window.set_selected_file("".into());
            }
            _ => {}
        }
    });

    // Audio Playback Stop/Start Logic -----------------------------------------
    {
        let audio_player_ref = Rc::clone(&player);
        let play_ptr = main_window.as_weak();
        main_window.on_toggle_play(move | new_state: bool | {
            let main_window = play_ptr.upgrade().unwrap();

            // Cant play if no file is selected
            if main_window.get_selected_file().trim().is_empty() {
                main_window.set_is_playing(false);
                return;
            }
            if let Some(ref mut player) = *audio_player_ref.borrow_mut() {
                if !new_state {
                    player.pause();
                } else {
                    player.start();
                }
            }
        });
    }

    // Parametric Eq Node Initialization logic ---------------------------------
    main_window.on_init_eq_nodes(move | n: i32 | {
        let mut nodes = Vec::new();
        let min_freq: f32 = 20.;
        let max_freq: f32 = 20000.;
        for i in 0..n {
            let freq = (min_freq * (max_freq / min_freq).powf(i as f32 / (n as f32 - 1.0))).round();
            nodes.push(NodeData { gain: (i * (-1i32).pow(i as u32)) as f32, freq, q: 1.0 });
        }
        return ModelRc::new(Rc::new(VecModel::from(nodes)));
    });

    // Draw Eq Image & set audio EQ
    let player_eq_ptr = Arc::clone(&player_eq);
    main_window.on_request_eq_response(move | 
        eq_nodes: ModelRc<NodeData>,
        low_freq_bound: f32, high_freq_bound: f32,
        min_gain: f32, max_gain: f32,
        imgx: f32, imgy: f32 | {
        let mut player_eq = player_eq_ptr.lock().unwrap();
        player_eq.reset();
        if let Some(nodes) = eq_nodes.as_any().downcast_ref::<VecModel<NodeData>>() {
            for n in nodes.iter() {
                player_eq.add_node(n.freq as u32, n.gain, n.q);
            }
        }

        generate_eq_response(player_eq.deref(), low_freq_bound as u32, high_freq_bound as u32, min_gain, max_gain, imgx as u32, imgy as u32)
    });

    // Slider-to-audio behaviour -----------------------------------------------
    {
        let audio_player_ref = Rc::clone(&player);
        main_window.on_slider_released(move | value: f32 | {
            if let Some(ref mut player) = *audio_player_ref.borrow_mut() {
                player.set_progress(value / 100.);
            }
        });
    }

    // Slider update logic -----------------------------------------------------
    let timer_ptr = main_window.as_weak();
    let audio_player_timer_ref = Rc::clone(&player);
    let timer = Timer::default();
    timer.start(TimerMode::Repeated, std::time::Duration::from_millis(50), move || {
        let main_window = timer_ptr.upgrade().unwrap();
        if let Some(ref mut player) = *audio_player_timer_ref.borrow_mut() {
            if player.is_finished() {
                main_window.set_is_playing(false);
                player.set_finished(false);
            }
            if !main_window.get_slider_pressed() {
                main_window.set_slider_pos(player.get_progress() * 100.);
            }
        }
    });

    // On Audio file select ----------------------------------------------------
    {
        let audio_player_ref = Rc::clone(&player);
        let file_sel_ptr = main_window.as_weak();
        let player_eq_ptr_fs = Arc::clone(&player_eq);
        main_window.on_file_select(move |file: SharedString| {

            *audio_player_ref.borrow_mut() = Some(AudioPlayer::new(file.into(), Arc::clone(&player_eq_ptr_fs)));
            let main_window = file_sel_ptr.upgrade().unwrap();
            let file_dur = audio_player_ref.borrow().as_ref().unwrap().duration;
            main_window.set_file_duration(file_dur);
        });
    }

    main_window.show()?;
    run_event_loop()?;

    //let menu_options = vec![
    //    MenuOption::new("Play Audio", || play_audio_menu()),
    //    MenuOption::new("Perform Short-Time DFT", || do_stdft_menu()),
    //    MenuOption::new("Create Spectrogram", || create_spectrogram_menu()),
    //    MenuOption::new("Create Waveform", || create_waveform_menu()),
    //];
    //
    //let mut menu = Menu::new(
    //    menu_options,
    //    MenuProps {
    //        title: ".Wav Parser",
    //        exit_on_action: false,
    //        message: "<esc> to close",
    //        ..menu_default_with_colors()
    //    },
    //);
    //
    //menu.show();

    Ok(())
}

//fn do_stdft(file_choice: &str) {
//    //clear the console output
//    print!("{}[2J", 27 as char);
//
//    let f = File::open(format!("./res/audio/{}", file_choice)).expect("Failed to open file!");
//    let mut reader = BufReader::new(f);
//    let file_info = file_io::read_wav_meta(&mut reader);
//    let sample_rate = file_info.sample_rate;
//    let channels = file_info.channels;
//
//    println!("Enter the start point of the Short-Time DFT (seconds)");
//    let start = read_stdin_f32(0., f32::MAX);
//    println!("Enter the duration of the Short-Time DFT (seconds)");
//    let duration = read_stdin_f32(f32::EPSILON, f32::MAX);
//    println!("Enter the window size (duration of each DFT)");
//    let window_size = read_stdin_f32(f32::EPSILON, duration);
//    println!("Enter the window overlap (percent, up to 90%)");
//    let overlap = read_stdin_usize(0, 90);
//    println!("Enter which window function to use: (0: Square, 1: Hann)");
//    let window_func = WindowFunction::from(read_stdin_usize(0, 1));
//    println!(
//        "Should the stdft be done on all {} channels of the file? (y/n)",
//        channels
//    );
//    let channel_choice;
//    let num_ch;
//    if read_stdin_bool() {
//        channel_choice = 0..=(channels as usize - 1);
//        num_ch = channels;
//    } else {
//        println!(
//            "Which channel should the stdft be done on? (0-{})",
//            channels
//        );
//        let choice = read_stdin_usize(0, channels as usize - 1);
//        channel_choice = choice..=choice;
//        num_ch = 1;
//    }
//    println!("Enter the filename for the resulting Short-Time DFT (without the extension)");
//    let mut dest_file = String::new();
//    stdin().read_line(&mut dest_file).unwrap();
//
//    let signal = read_data(&mut reader, file_info, start, duration).unwrap();
//    let original_signal = SignalPlayer::new(signal, sample_rate, channels as usize);
//
//    println!("Starting {} Short-Time DFTs...\n------", num_ch);
//    for i in channel_choice {
//        println!("Performing Short-Time DFT #{}...", i);
//        let stdft = original_signal.do_short_time_fourier_transform(
//            window_size,
//            overlap as f32 / 100.,
//            window_func,
//            i,
//        );
//        println!("Short-Time DFT #{} Complete!", i);
//
//        println!("Writing stdft #{} to File...", i);
//        write_stdft_to_file(
//            format!("./res/stdfts/{}_ch{}.stdft", dest_file.trim(), i),
//            &stdft,
//        );
//        println!("Done #{}!", i);
//    }
//    println!("Done!");
//    thread::sleep(Duration::from_millis(500));
//}
//
//fn play_audio_menu() {
//    let files = query_directory("./res/audio");
//
//    let mut menu_options: Vec<MenuOption> = vec![];
//
//    // todo: need to have some way of figuring out which sample rates i can use
//    // and which files are possible to play based on that
//    // then also provide functionality to resample files
//    for file in files {
//        menu_options.push(MenuOption::new(file.clone().as_str(), move || {
//            let mut r = BufReader::new(File::open(format!("./res/audio/{}", &file)).unwrap());
//            let meta = read_wav_meta(&mut r);
//            match meta.sample_type {
//                1 => play_audio_direct(&file, meta),
//                _ => play_audio_stream(meta, &mut r),
//            }
//        }));
//    }
//
//    let mut audio_file_menu = Menu::new(
//        menu_options,
//        MenuProps {
//            title: "Choose a File:",
//            message: "<esc> to close",
//            ..menu_default_with_colors()
//        },
//    );
//    audio_file_menu.show();
//}
//
//fn play_audio_stream(file_meta: WavInfo, reader: &mut BufReader<File>) {
//    println!("Audio player starting");
//    let signal_player = Arc::new(Mutex::new(SignalPlayer::new(
//        read_data(reader, file_meta.clone(), 0., file_meta.audio_duration).unwrap(),
//        file_meta.sample_rate,
//        file_meta.channels as usize,
//    )));
//
//    let host: Host = cpal::default_host();
//    let device: Device = host
//        .default_output_device()
//        .expect("No audio output device available!");
//
//    let mut supported_stream_range = device
//        .supported_output_configs()
//        .expect("Error while querying output configs!");
//    let supported_config: StreamConfig = supported_stream_range
//        .find(|&e| e.max_sample_rate() == SampleRate(48000))
//        .expect("No supported configs!")
//        .with_sample_rate(SampleRate(48000))
//        .config();
//
//    let signal_player_clone = Arc::clone(&signal_player);
//    let stream = device
//        .build_output_stream_raw(
//            &supported_config,
//            cpal::SampleFormat::F32,
//            move |data: &mut Data, _: &OutputCallbackInfo| {
//                signal_player_clone.lock().unwrap().next_chunk(data);
//            },
//            move |_err| {
//                panic!("bad things happened");
//            },
//            None,
//        )
//        .unwrap();
//    println!("Playing Audio...");
//    stream.play().unwrap();
//
//    // keep track of whether the audio has finished playing on the current thread
//    loop {
//        if signal_player.lock().unwrap().finished == true {
//            println!("Audio playback complete!");
//
//            sleep(Duration::from_millis(500));
//            return;
//        }
//        sleep(Duration::from_millis(100));
//    }
//}
//
//fn play_audio_direct(file_path: &str, file_meta: WavInfo) {
//    println!("Audio player starting");
//    let file_player = Arc::new(Mutex::new(FilePlayer::new(file_path.into())));
//
//    let host: Host = cpal::default_host();
//    let device: Device = host
//        .default_output_device()
//        .expect("No audio output device available!");
//
//    let mut supported_stream_range = device
//        .supported_output_configs()
//        .expect("Error while querying output configs!");
//    let supported_config: StreamConfig = supported_stream_range
//        .find(|&e| e.max_sample_rate() == SampleRate(48000))
//        .expect("No supported configs!")
//        .with_sample_rate(SampleRate(file_meta.sample_rate))
//        .config();
//
//    let file_player_clone = Arc::clone(&file_player);
//    let stream = device
//        .build_output_stream_raw(
//            &supported_config,
//            cpal::SampleFormat::F32,
//            move |data: &mut Data, _: &OutputCallbackInfo| {
//                file_player_clone.lock().unwrap().next_chunk(data);
//            },
//            move |_err| {
//                panic!("bad things happened");
//            },
//            None,
//        )
//        .unwrap();
//    println!("Playing Audio...");
//    stream.play().unwrap();
//
//    // keep track of whether the audio has finished playing on the current thread
//    loop {
//        if file_player.lock().unwrap().finished == true {
//            println!("Audio playback complete!");
//            return;
//        }
//        sleep(Duration::from_millis(1000));
//    }
//}
//
//fn create_spectrogram_menu() {
//    let files = query_directory("./res/stdfts");
//
//    let mut menu_options: Vec<MenuOption> = vec![];
//    for file in files {
//        menu_options.push(MenuOption::new(file.clone().as_str(), move || {
//            generate_spectrogram(&file)
//        }));
//    }
//
//    let mut stdft_file_menu = Menu::new(
//        menu_options,
//        MenuProps {
//            title: "Choose a File:",
//            message: "<esc> to close",
//            ..menu_default_with_colors()
//        },
//    );
//    stdft_file_menu.show();
//}
//
//fn generate_spectrogram(dir: &str) {
//    print!("{}[2J", 27 as char);
//
//    println!("Enter the width of the image:");
//    let imgx = read_stdin_u32();
//    println!("Enter the height of the image:");
//    let imgy = read_stdin_u32();
//    println!("Enter the file name of the resulting image (without the extension)");
//    let mut dest_file = String::new();
//    stdin()
//        .read_line(&mut dest_file)
//        .expect("Failed to read input");
//
//    println!("Reading Data from file...");
//    let stdft = read_stdft_from_file(("./res/stdfts/".to_string() + dir).as_str());
//
//    println!("Generating image...");
//    generate_spectrogram_img(
//        format!("./res/spectrograms/{}.png", dest_file.trim()),
//        imgx,
//        imgy,
//        stdft,
//    )
//    .expect("Failed to save image!");
//    println!("Done!");
//    thread::sleep(Duration::from_millis(500));
//}
//
//
//fn create_waveform_menu() {
//    let files = query_directory("./res/audio");
//
//    let mut menu_options: Vec<MenuOption> = vec![];
//    for file in files {
//        menu_options.push(MenuOption::new(file.clone().as_str(), move || {
//            generate_waveform(&file)
//        }));
//    }
//
//    let mut stdft_file_menu = Menu::new(
//        menu_options,
//        MenuProps {
//            title: "Choose a File:",
//            message: "<esc> to close",
//            ..menu_default_with_colors()
//        },
//    );
//    stdft_file_menu.show();
//}
//
//fn generate_waveform(dir: &str) {
//    print!("{}[2J", 27 as char);
//
//    println!("Enter the width of the image:");
//    let imgx = read_stdin_u32();
//    println!("Enter the height of the image:");
//    let imgy = read_stdin_u32();
//    println!("Enter the file name of the resulting image (without the extension)");
//    let mut dest_file = String::new();
//    stdin()
//        .read_line(&mut dest_file)
//        .expect("Failed to read input");
//
//    let mut f =
//        BufReader::new(File::open(format!("./res/audio/{}", dir)).expect("Failed to open file!"));
//    let file_info = file_io::read_wav_meta(&mut f);
//
//    println!("Generating image...");
//    generate_waveform_img(
//        format!("./res/waveforms/{}.png", dest_file.trim()),
//        imgx,
//        imgy,
//        read_data(&mut f, file_info.clone(), 0., file_info.audio_duration).unwrap(),
//    )
//    .unwrap();
//
//    println!("Done!");
//    thread::sleep(Duration::from_millis(500));
//}
//
//fn menu_default_with_colors() -> MenuProps<'static> {
//    MenuProps {
//        bg_color: 248,
//        fg_color: 19,
//        msg_color: Some(244),
//        title_color: Some(17),
//        selected_color: Some(21),
//        ..MenuProps::default()
//    }
//}
