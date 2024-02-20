use std::cell::{Ref};
// use emulator_6502::Interface6502;
use rand::{Rng, thread_rng};
use w65c02s::{System, W65C02S};
use super::registers::*;

#[derive(Debug)]
pub struct Bus {
    pub zero_page: [u8; 0x100],
    pub cpu_stack: [u8; 0x100],

    pub system_control: SystemControl,
    pub blitter: BlitterRegisters,
    pub ram_banks: Box<[[u8; 0x2000 - 0x200]; 4]>,
    pub framebuffers: [SharedFrameBuffer; 2],
    pub vram_banks: Box<[[[u8; 128*128]; 4]; 8]>,
    pub cartridge: Cartridge32k,
}

const MICROVOID_GTR: &[u8] = include_bytes!("../test_cartridges/microvoid.gtr");

impl Default for Bus {
    fn default() -> Self {
        let mut rng = thread_rng();

        let mut bus = Self {
            zero_page: [0; 0x100],
            cpu_stack: [0; 0x100],
            system_control: SystemControl {
                reset_acp: 0,
                nmi_acp: 0,
                banking_register: BankingRegister(0),
                audio_enable_sample_rate: 0,
                dma_flags: BlitterFlags(0b0000_1000),
                gamepad_1: 0,
                gamepad_2: 0,
            },
            blitter: BlitterRegisters {
                vx: 0,
                vy: 0,
                gx: 0,
                gy: 0,
                width: 127,
                height: 127,
                start: 0,
                color: 0b101_00_000, // offwhite
            },
            ram_banks: Box::new([[0; 0x2000 - 0x200]; 4]),
            framebuffers: [new_framebuffer(0x00), new_framebuffer(0xFF)],
            vram_banks: Box::new([[[0; 128*128]; 4]; 8]),
            cartridge: Cartridge32k {
                data: Box::new([0; 0x8000]),
            },
        };

        for (idx, byte) in MICROVOID_GTR.iter().enumerate() {
            bus.cartridge.data[idx] = *byte;
            // print!("{:02X} ", byte);
        }
        // println!();

        for p in bus.framebuffers[0].borrow_mut().iter_mut() {
            *p = rng.gen();
        }

        for p in bus.framebuffers[1].borrow_mut().iter_mut() {
            *p = rng.gen();
        }

        bus
    }
}

impl Bus {
    pub fn read_full_framebuffer(&self) -> Ref<'_, FrameBuffer> {
        let fb = self.system_control.get_framebuffer_out();
        return self.framebuffers[fb].borrow();
    }
    pub fn write_byte(&mut self, address: u16, data: u8) {
        match address {
            // zero page
            0x0000..=0x00FF => {
                self.zero_page[address as usize] = data;
            }

            // cpu stack
            0x0100..=0x01FF => {
                self.cpu_stack[address as usize - 0x100] = data;
            }

            // system RAM
            0x0200..=0x1FFF => {
                self.ram_banks[self.system_control.get_ram_bank()][address as usize - 0x200] = data;
                // println!("${:04X}={:02X}", address, data);
            }

            // system control registers
            0x2000..=0x2009 => {
                self.system_control.write_byte(address, data);
                // println!("${:04X}={:08b}", address, data);
            }

            // versatile interface adapter (GPIO, timers)
            0x2800..=0x280F => {
                // TODO: unimplemented
            }

            // audio RAM
            0x3000..=0x3FFF => {
                // TODO: unimplemented
            }

            // VRAM/Framebuffer/Blitter
            0x4000..=0x7FFF => {
                match self.system_control.get_graphics_memory_map() {
                    GraphicsMemoryMap::FrameBuffer => {
                        let fb = self.system_control.banking_register.framebuffer() as usize;
                        self.framebuffers[fb].borrow_mut()[address as usize - 0x4000] = data;
                    }
                    GraphicsMemoryMap::VRAM => {
                        let vram_page = self.system_control.banking_register.vram_page() as usize;
                        let quadrant = self.blitter.vram_quadrant();
                        self.vram_banks[vram_page][quadrant][address as usize - 0x4000] = data;
                    }
                    GraphicsMemoryMap::BlitterRegisters => {
                        self.blitter.write_byte(address, data);
                        // println!("blitter reg write -> ${:04X}={:02X}", address, data);
                    }
                }
            }
            _ => {
                log::warn!("Attempted to write read-only memory at: ${:02X}", address);
            }
        }
    }

    pub fn read_byte(&mut self, address: u16) -> u8 {
        match address {
            // zero page
            0x0000..=0x00FF => {
                return self.zero_page[address as usize];
            }

            // cpu stack
            0x0100..=0x01FF => {
                return self.cpu_stack[address as usize - 0x100];
            }

            // system RAM
            0x0200..=0x1FFF => {
                return self.ram_banks[self.system_control.get_ram_bank()][address as usize - 0x200];
            }

            // system control registers
            0x2000..=0x2009 => {
                return self.system_control.read_byte(address);
            }

            // versatile interface adapter (GPIO, timers)
            0x2800..=0x280F => {
                // TODO: unimplemented
            }

            // audio RAM
            0x3000..=0x3FFF => {
                // TODO: unimplemented
            }

            // VRAM/Framebuffer/Blitter
            0x4000..=0x7FFF => {
                match self.system_control.get_graphics_memory_map() {
                    GraphicsMemoryMap::FrameBuffer => {
                        let fb = self.system_control.banking_register.framebuffer() as usize;
                        return self.framebuffers[fb].borrow()[address as usize - 0x4000];
                    }
                    GraphicsMemoryMap::VRAM => {
                        let vram_page = self.system_control.banking_register.vram_page() as usize;
                        let quadrant = self.blitter.vram_quadrant();
                        return self.vram_banks[vram_page][quadrant][address as usize - 0x4000];
                    }
                    GraphicsMemoryMap::BlitterRegisters => {
                        return self.blitter.read_byte(address);
                    }
                }
            }

            // Cartridge
            0x8000..=0xFFFF => {
                return self.cartridge.data[address as usize - 0x8000]
            }
            _ => {
                log::warn!("Attempted to inaccessible memory at: ${:02X}", address);
            }
        }

        0
    }
}

impl System for Bus {
    fn read(&mut self, _: &mut W65C02S, addr: u16) -> u8 {
        self.read_byte(addr)
    }

    fn write(&mut self, _: &mut W65C02S, addr: u16, data: u8) {
        self.write_byte(addr, data);
    }
}

// impl Interface6502 for Bus {
//     fn read(&mut self, addr: u16) -> u8 { self.read_byte(addr) }
//
//     fn write(&mut self, addr: u16, data: u8) { self.write_byte(addr, data); }
// }
