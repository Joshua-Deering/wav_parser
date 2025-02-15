use std::f32::consts::TAU;
use crate::util::logspace;

pub struct ParametricEq {
    nodes: Vec<Biquad>,
    sample_rate: u32,
}

impl ParametricEq {
    pub fn new(nodes: Vec<Biquad>, sample_rate: u32) -> Self {

        Self {
            nodes,
            sample_rate,
        }
    }
    
    pub fn reset(&mut self) {
        self.nodes = vec![];
    }

    pub fn add_biquad(&mut self, node: Biquad) {
        self.nodes.push(node);
    }

    pub fn add_node(&mut self, freq: u32, gain: f32, q: f32) {
        self.nodes.push(Biquad::new(freq, gain, q, self.sample_rate));
    }

    pub fn process(&mut self, samples: &mut [f32]) {
        for i in 0..samples.len() {
            for filter in &mut self.nodes {
                samples[i] = filter.process(samples[i])
            }
        }
    }

    pub fn get_freq_response(&self, lower_bound: u32, upper_bound: u32, num_points: usize) -> Vec<(u32, f32)> {
        let freq_step = (upper_bound - lower_bound) as usize / num_points;
        let mut test_pts = vec![(0, 0.); num_points as usize];
        for (test_freq, i) in (lower_bound..=upper_bound).step_by(freq_step).zip(0..num_points) {
            let mut sum = 0.;
            for node in &self.nodes {
                sum += (20. * node.calc_response(test_freq as f32).log10()) - node.ref_value;
            }
            test_pts[i] = (test_freq, sum);
        }

        test_pts
    }

    pub fn get_freq_response_log(&self, lower_bound: u32, upper_bound: u32, num_points: usize) -> Vec<(u32, f32)> {
        let mut test_pts = vec![(0, 0.); num_points as usize];
        for (test_freq, i) in logspace(lower_bound, upper_bound, num_points).zip(0..num_points) {
            let mut sum = 0.;
            for node in &self.nodes {
                sum += (20. * node.calc_response(test_freq as f32).log10()) - node.ref_value;
            }
            test_pts[i] = (test_freq as u32, sum);
        }

        test_pts
    }
}

pub enum FilterType {
    PEAKING,
    LPF,
    HPF,
//    BPF,
    NOTCH,
    LOWSHELF,
    HIGHSHELF
}

pub struct EqNode {
    node_type: FilterType,
    freq: f32,
    gain: f32,
    q: f32,
}

impl EqNode { 
    pub fn new(freq: f32, gain: f32, q: f32, node_type: FilterType) -> Self {
        Self { node_type, freq, gain, q }
    }
}

pub struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
    sample_rate: u32,
    pub ref_value: f32,
}

impl Biquad {
    pub fn new(frequency: u32, gain: f32, q: f32, sample_rate: u32) -> Self {
        let omega = TAU * frequency as f32 / sample_rate as f32;
        let alpha = omega.sin() / (2. * q);
        let a_gain = 10f32.powf(gain / 40.);

        let b0 = 1. + alpha * a_gain;
        let b1 = -2. * omega.cos();
        let b2 = 1. - alpha * a_gain;
        let a0 = 1. + alpha / a_gain;
        let a1 = -2. * omega.cos();
        let a2 = 1. - alpha / a_gain;

        let mut out = Self {
            b0: b0/a0,
            b1: b1/a0,
            b2: b2/a0,
            a1: a1/a0,
            a2: a2/a0,
            z1: 0.,
            z2: 0.,
            sample_rate,
            ref_value: 0.,
        };
        out.find_ref_value();
        out
    }

    pub fn with_coefficients(b0: f32, b1: f32, b2: f32, a1: f32, a2: f32, sample_rate: u32) -> Self {
        let mut out = Self {
            b0, b1, b2, a1, a2,
            z1: 0.,
            z2: 0.,
            sample_rate,
            ref_value: 0.
        };
        out.find_ref_value();
        out
    }

    fn find_ref_value(&mut self) {
        // take samples along frequency range, finding the value closest to unity gain 
        // (used for reference when graphing the frequency response of an eq)
        let mut closest_freq = 0;
        let mut best_diff = f32::MAX;
        for freq in (0..self.sample_rate/2).step_by((self.sample_rate/2) as usize / 1000) {
            let diff = (1. - self.calc_response(freq as f32)).abs();
            if diff < best_diff {
                closest_freq = freq;
                best_diff = diff;
            }
        }

        self.ref_value = 20. * self.calc_response(closest_freq as f32).log10();
    } 

    //process a single sample
    fn process(&mut self, sample: f32) -> f32 {
        let out = self.b0 * sample + self.z1;
        self.z1 = self.b1 * sample + self.z2 - self.a1 * out;
        self.z2 = self.b2 * sample - self.a2 * out;
        out
    }

    // calculate the frequency response for a single frequency
    fn calc_response(&self, frequency: f32) -> f32 {
        let omega = TAU * frequency as f32 / self.sample_rate as f32;
        let sin_omega = omega.sin();
        let cos_omega = omega.cos();

        let numerator = (self.b0 + self.b1 * cos_omega + self.b2 * (2.0 * cos_omega.powi(2) - 1.0)).powi(2)
            + (self.b1 * sin_omega + self.b2 * 2.0 * cos_omega * sin_omega).powi(2);

        let denominator = (1.0 + self.a1 * cos_omega + self.a2 * (2.0 * cos_omega.powi(2) - 1.0)).powi(2)
            + (self.a1 * sin_omega + self.a2 * 2.0 * cos_omega * sin_omega).powi(2);

        (numerator / denominator).sqrt()
    }
}
