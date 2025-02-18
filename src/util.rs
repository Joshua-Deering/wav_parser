use std::{fs, io};
use std::ops::Range;

use plotters::coord::ranged1d::AsRangedCoord;

#[allow(unused)]
pub fn get_arr_from_slice(slice: &[u8]) -> [u8; 4] {
    if slice.len() > 4 {
        panic!("attempted to convert slice of size {} to size 4", slice.len());
    }
    let mut out = [0; 4];
    for i in 0..slice.len() {
        out[i] = slice[i];
    }
    out
}

pub fn query_directory(dir: &str) -> impl Iterator<Item = String> {
    let mut entries = fs::read_dir(dir)
        .unwrap()
        .map(|res| res.map(|e| e.file_name()))
        .collect::<Result<Vec<_>, io::Error>>()
        .unwrap();
    entries.retain(|e| !e.eq_ignore_ascii_case(".DS_store"));

    entries.into_iter().map(|e| e.to_string_lossy().to_string())
}

pub fn logspace(min: f32, max: f32, num_points: usize) -> impl Iterator<Item = f32> {
    let log_min = min.ln();
    let log_max = max.ln();
    let step = (log_max - log_min) / ((num_points - 1) as f32);
    
    (0..num_points).map(move |i| (log_min + step * (i as f32)).exp())
}

#[allow(unused)]
pub fn hue_to_rgb(h: f32, s: f32, v: f32) -> [u8; 3] {
    let c = v * s;
    let h = h / 60.;
    let x = c * (1. - f32::abs(h % 2. - 1.));
    let m = v - c;

    let rgb1: (f32, f32, f32);

    if h <= 1. {
        rgb1 = (c, x, 0.);
    } else if h <= 2. {
        rgb1 = (x, c, 0.);
    } else if h <= 3. {
        rgb1 = (0., c, x);
    } else if h <= 4. {
        rgb1 = (0., x, c);
    } else if h <= 5. {
        rgb1 = (x, 0., c);
    } else {
        rgb1 = (c, 0., x);
    }

    [
        ((rgb1.0 + m) * 255.) as u8,
        ((rgb1.1 + m) * 255.) as u8,
        ((rgb1.2 + m) * 255.) as u8,
    ]
}

//pub fn read_stdin_bool() -> bool {
//    let mut inp = String::new();
//    loop {
//        stdin().read_line(&mut inp).expect("Failed to read stdin");
//
//        match inp.to_lowercase().trim() {
//            "y" | "yes" | "true" | "t" | "1" => {
//                return true;
//            }
//            "n" | "no" | "false" | "f" | "0" => {
//                return false;
//            }
//            _ => {
//                println!("Invalid Input! Please only enter yes or no (or other aliases like y/n, t/f, etc)")
//            }
//        }
//        inp.clear();
//    }
//}
//
//pub fn read_stdin_u32() -> u32 {
//    let mut inp = String::new();
//    loop {
//        stdin().read_line(&mut inp).expect("Failed to read stdin");
//
//        if let Ok(num) = inp.trim().parse::<u32>() {
//            return num;
//        } else {
//            println!("Invalid input! Please only enter valid positive integers");
//        }
//        inp.clear();
//    }
//}
//
//pub fn read_stdin_usize(min: usize, max: usize) -> usize {
//    let mut inp = String::new();
//    loop {
//        stdin().read_line(&mut inp).expect("Failed to read stdin");
//
//        if let Ok(num) = inp.trim().parse::<usize>() {
//            if num < min || num > max {
//                println!("Please only enter numbers in the range! ({} to {})", min, max);
//            } else {
//                return num;
//            }
//        } else {
//            println!("Invalid input! Please only enter valid positive integers");
//        }
//        inp.clear();
//    }
//}
//
//pub fn read_stdin_f32(min: f32, max: f32) -> f32 {
//    let mut inp = String::new();
//    loop {
//        stdin().read_line(&mut inp).expect("Failed to read stdin");
//
//        if let Ok(num) = inp.trim().parse::<f32>() {
//            if num < min || num > max {
//                println!("Please only enter numbers in the range! ({:.2e} to {:.2e})", min, max);
//            } else {
//                return num;
//            }
//        } else {
//            println!("Invalid input! Please only enter valid positive integers");
//        }
//        inp.clear();
//    }
//}
