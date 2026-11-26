//! The D-Bus subset the AT-SPI backend speaks: addresses, the wire
//! format and, on Linux, a blocking client connection.

pub mod address;
#[cfg(target_os = "linux")]
pub mod connection;
pub mod message;
mod signature;
pub mod value;

#[cfg(test)]
mod tests;
