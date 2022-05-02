pub mod client;
pub mod server;

use rand::{Rng, thread_rng};
use rand::distributions::Alphanumeric;

fn random_string(len: usize) -> String {
    thread_rng().sample_iter(&Alphanumeric).take(len).map(char::from).collect()
}