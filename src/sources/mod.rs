//! Where package data comes from. Nothing here writes to the system.

pub mod aur;
pub mod pacman;

#[cfg(test)]
mod pacman_tests;
