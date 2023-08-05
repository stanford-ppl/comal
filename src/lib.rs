#![allow(dead_code)]

pub mod config;
pub mod templates;

pub mod tortilla {
    include!(concat!(env!("OUT_DIR"), "/tortilla.rs"));
}
