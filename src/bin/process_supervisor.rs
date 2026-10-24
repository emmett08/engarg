//! Installed companion facade to the sole EAL process supervisor implementation.
fn main() {
    std::process::exit(eal_process::supervisor::entrypoint());
}
