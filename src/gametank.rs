use std::cell::{Ref, RefCell};
use bitfield::bitfield;
// use emulator_6502::Interface6502;
use rand::{Rng, thread_rng};
use w65c02s::{System, W65C02S};

bitfield!{
    pub struct BankingRegister(u8);
    impl Debug;
    pub vram_page, set_ram_page: 2, 0;
    pub framebuffer, set_framebuffer: 3;
    pub clip_blits_h, set_clip_blits_h: 4;
    pub clip_blits_v, set_clip_blits_v: 5;
    pub ram_bank, set_ram_bank: 7, 6;
}

bitfield!{
    pub struct BlitterFlags(u8);
    impl Debug;
    pub dma_enable, set_dma_enable : 0;
    pub dma_page_out, set_dma_page_out : 1;
    pub dma_nmi, set_dma_nmi : 2;
    pub dma_colorfill_enable, set_dma_colorfill_enable : 3;
    pub dma_gcarry, set_dma_gcarry : 4;
    pub dma_cpu_to_vram, set_dma_cpu_to_vram : 5;
    pub dma_irq, set_dma_irq : 6;
    pub dma_opaque, set_dma_opaque : 7;
}


#[derive(Debug)]
pub struct BlitterRegisters {
    pub vx: u8,
    pub vy: u8,
    pub gx: u8,
    pub gy: u8,
    pub width: u8,
    pub height: u8,
    pub start: u8,
    pub color: u8,
}

impl BlitterRegisters {
    pub fn vram_quadrant(&self) -> usize {
        let mut quadrant = 0;

        if self.gx >= 128 {
            quadrant += 1
        }

        if self.gy >= 128 {
            quadrant += 2
        }

        quadrant
    }

    fn read_byte(&self, address: u16) -> u8 {
        log::warn!("Attempted to read from unreadable memory at: ${:02X}", address);
        0
    }

    fn write_byte(&mut self, address: u16, data: u8) {
        match address {
            0x4000 => { self.vx = data }
            0x4001 => { self.vy = data }
            0x4002 => { self.gx = data }
            0x4003 => { self.gy = data }
            0x4004 => { self.width = data }
            0x4005 => { self.height = data }
            0x4006 => { self.start = data }
            0x4007 => { self.color = data }
            _ => {}
        }
    }
}

#[derive(Debug)]
pub enum GraphicsMemoryMap {
    FrameBuffer,
    VRAM,
    BlitterRegisters
}

#[derive(Debug)]
pub struct SystemControl {
    pub reset_acp: u8,
    pub nmi_acp: u8,

    // has effects on the rest of the system
    pub banking_register: BankingRegister,

    pub audio_enable_sample_rate: u8,
    pub dma_flags: BlitterFlags,
    pub gamepad_1: u8,
    pub gamepad_2: u8,
}

impl SystemControl {
    pub fn get_ram_bank(&self) -> usize {
        self.banking_register.ram_bank() as usize
    }

    pub fn get_graphics_memory_map(&self) -> GraphicsMemoryMap {
        if self.dma_flags.dma_enable() { // 1 is blitter enabled
            return GraphicsMemoryMap::BlitterRegisters
        }

        if self.dma_flags.dma_cpu_to_vram() {
            return GraphicsMemoryMap::FrameBuffer
        }

        return GraphicsMemoryMap::VRAM
    }

    pub fn get_framebuffer_out(&self) -> usize {
        self.dma_flags.dma_page_out() as usize
    }

    fn write_byte(&mut self, address: u16, data: u8) {
        match address {
            0x2000 => { self.reset_acp = data } // TODO: reset acp
            0x2001 => { self.nmi_acp = data } // TODO: nmi acp
            0x2005 => { self.banking_register.0 = data }
            0x2006 => { self.audio_enable_sample_rate = data } // TODO: ???
            0x2007 => { self.dma_flags.0 = data }
            _ => {
                log::warn!("Attempted to write read-only memory at: ${:02X}", address);
            }
        }
    }

    fn read_byte(&mut self, address: u16) -> u8 {
        match address {
            0x2008 => { 0 } // TODO: read inputs and track controller state
            0x2009 => { 0 } // TODO: read inputs and track controller state
            _ => {
                log::warn!("Attempted to read from unreadable memory at: ${:02X}", address);
                0
            }
        }
    }
}

#[derive(Debug)]
pub struct Cartridge32k {
    pub data: Box<[u8; 0x8000]>
}


type FrameBuffer = Box<[u8; 128*128]>;
type SharedFrameBuffer = RefCell<FrameBuffer>;

fn new_framebuffer(fill: u8) -> SharedFrameBuffer {
    RefCell::new(Box::new([fill; 128*128]))
}

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

const MICROVOID_GTR: &[u8] = include_bytes!("microvoid.gtr");

impl Bus {
    pub fn default() -> Self {
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
