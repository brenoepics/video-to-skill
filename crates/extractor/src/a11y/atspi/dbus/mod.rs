//! The D-Bus subset the AT-SPI backend speaks: addresses, the wire
//! format and, on Linux, a blocking client connection.

pub mod address;
#[cfg(target_os = "linux")]
pub mod connection;
pub mod message;
mod signature;
#[cfg(target_os = "linux")]
mod transport;
pub mod value;

#[cfg(test)]
mod hardening_tests;
#[cfg(test)]
mod tests;
