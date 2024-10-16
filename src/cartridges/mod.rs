mod cart32k;
mod cart8k;
mod cart2m;

use std::ops::{Deref, DerefMut};
use crate::cartridges::cart2m::Cartridge2M;
use crate::cartridges::cart32k::{Cartridge32K};
use crate::cartridges::cart8k::Cartridge8K;

pub trait Cartridge: Deref<Target = [u8; 0x8000]> + DerefMut {
    fn from_slice(slice: &[u8]) -> Self;
}

pub enum CartridgeType {
    Cart32k(Cartridge32K),
    Cart8k(Cartridge8K),
    Cart2m(Box<Cartridge2M>),
}

impl Deref for CartridgeType {
    type Target = [u8; 0x8000];

    fn deref(&self) -> &Self::Target {
        match self {
            CartridgeType::Cart32k(inner) => {&inner}
            CartridgeType::Cart8k(inner) => {&inner}
            CartridgeType::Cart2m(inner) => {inner}
        }
    }
}

impl DerefMut for CartridgeType {
    fn deref_mut(&mut self) -> &mut Self::Target {
        match self {
            CartridgeType::Cart32k(inner) => {inner}
            CartridgeType::Cart8k(inner) => {inner}
            CartridgeType::Cart2m(inner) => {inner}
        }
    }
}

impl CartridgeType {
    pub fn from_slice(slice: &[u8]) -> Self {
        match slice.len() {
            0x2000 => {
                CartridgeType::Cart8k(Cartridge8K::from_slice(slice))
            }
            0x8000 => {
                CartridgeType::Cart32k(Cartridge32K::from_slice(slice))
            }
            0x200000 => {
                CartridgeType::Cart2m(Box::new(Cartridge2M::from_slice(slice)))
            }
            _ => {
                panic!("unimplemented");
            }
        }
    }
}

//
// fn from_slice(slice: &[u8]) -> Box<dyn Cartridge> {
//
// }
