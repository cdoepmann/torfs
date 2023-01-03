use std::cell::UnsafeCell;
use std::rc::Rc;
use std::sync::Mutex;

use lazy_static::lazy_static;
use rand::{CryptoRng, RngCore, SeedableRng};
use rand_chacha::ChaChaRng;
use sha1::{Digest, Sha1};

struct GlobalState {
    seed: u64,
    num_threads: u64,
}

impl GlobalState {
    fn new() -> GlobalState {
        GlobalState {
            seed: 0,
            num_threads: 0,
        }
    }
}

lazy_static! {
    static ref GLOBAL_STATE: Mutex<GlobalState> = Mutex::new(GlobalState::new());
}

/// Use "real" randomness to generate a random seed (using `thread_rng()`).
#[allow(dead_code)]
pub fn generate_random_seed() -> u64 {
    loop {
        let seed = rand::random();
        if seed != 0 {
            return seed;
        }
    }
}

#[allow(dead_code)]
pub fn set_seed(seed: u64) {
    let mut global_state = GLOBAL_STATE.lock().unwrap();
    global_state.seed = seed;
}

fn get_next_seed() -> u64 {
    let mut global_state = GLOBAL_STATE.lock().unwrap();
    global_state.num_threads += 1;

    // println!(
    //     "Used seed: {}, used num_threads: {}",
    //     global_state.seed, global_state.num_threads
    // );

    let seed = global_state.seed;
    let thread_num = global_state.num_threads;
    drop(global_state);

    // create a new seed out of these two values
    let mut hasher = Sha1::new();
    hasher.update(seed.to_be_bytes());
    hasher.update(thread_num.to_be_bytes());
    let digest = hasher.finalize();

    u64::from_be_bytes(digest[..8].try_into().unwrap())
}

// Largely inspiree/copied from rand's implementation of thread_rng...

thread_local!(
    static THREAD_SEEDED_RNG: Rc<UnsafeCell<ChaChaRng>> = {
        Rc::new(UnsafeCell::new(ChaChaRng::seed_from_u64(get_next_seed())))
    }
);

pub fn get_rng() -> SeededRng {
    let rng = THREAD_SEEDED_RNG.with(|t| t.clone());
    SeededRng { rng }
}

#[derive(Clone, Debug)]
pub struct SeededRng {
    rng: Rc<UnsafeCell<ChaChaRng>>,
}

impl Default for SeededRng {
    fn default() -> SeededRng {
        get_rng()
    }
}

impl RngCore for SeededRng {
    #[inline(always)]
    fn next_u32(&mut self) -> u32 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u32()
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.next_u64()
    }

    fn fill_bytes(&mut self, dest: &mut [u8]) {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.fill_bytes(dest)
    }

    fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), rand::Error> {
        // SAFETY: We must make sure to stop using `rng` before anyone else
        // creates another mutable reference
        let rng = unsafe { &mut *self.rng.get() };
        rng.try_fill_bytes(dest)
    }
}

impl CryptoRng for SeededRng {}
