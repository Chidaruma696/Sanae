//! Where package data comes from. Nothing here writes to the system.

pub mod appstream;
pub mod aur;
pub mod news;
pub mod pacman;
pub mod pkgstats;

#[cfg(test)]
mod pacman_tests;
