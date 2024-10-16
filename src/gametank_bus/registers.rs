use std::cell::RefCell;
use bitfield::bitfield;
use tracing::warn;
use crate::emulator::InputCommand;
use crate::gamepad::GamePad;

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

    pub fn read_byte(&self, address: u16) -> u8 {
        warn!("Attempted to read from unreadable memory at: ${:02X}", address);
        0
    }

    pub fn write_byte(&mut self, address: u16, data: u8) {
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

    pub gamepads: [GamePad; 2]
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
    
    pub fn acp_enabled(&self) -> bool {
        (self.audio_enable_sample_rate & 0b1000_0000) != 0
    }
    
    pub fn sample_rate(&self) -> u8 {
        self.audio_enable_sample_rate
    }

    pub fn get_framebuffer_out(&self) -> usize {
        self.dma_flags.dma_page_out() as usize
    }

    pub fn write_byte(&mut self, address: u16, data: u8) {
        match address {
            0x2000 => { self.reset_acp = data } // TODO: reset acp
            0x2001 => { self.nmi_acp = data } // TODO: nmi acp
            0x2005 => { self.banking_register.0 = data }
            0x2006 => { self.audio_enable_sample_rate = data } // TODO: ???
            0x2007 => { self.dma_flags.0 = data }
            _ => {
                warn!("Attempted to write read-only memory at: ${:02X}", address);
            }
        }
    }

    pub fn read_byte(&mut self, address: u16) -> u8 {

        match address {
            0x2008 => {
                self.read_gamepad_byte(true)
            }
            0x2009 => {
                self.read_gamepad_byte(false)
            }
            _ => {
                warn!("Attempted to read from unreadable memory at: ${:02X}", address);
                0
            }
        }
    }

    pub fn read_gamepad_byte(&mut self, port_1: bool) -> u8 {
        let gamepad = &mut self.gamepads[(!port_1) as usize];
        let mut byte = 255;
        if !gamepad.port_select {
            byte &= !((gamepad.start as u8) << 5);
            byte &= !((gamepad.a as u8) << 4);
        } else {
            byte &= !((gamepad.c as u8) << 5);
            byte &= !((gamepad.b as u8) << 4);
            byte &= !((gamepad.up as u8) << 3);
            byte &= !((gamepad.down as u8) << 2);
            byte &= !((gamepad.left as u8) << 1);
            byte &= !((gamepad.right as u8) << 0);
        }

        self.gamepads[port_1 as usize].port_select = false;
        self.gamepads[(!port_1) as usize].port_select = !self.gamepads[(!port_1) as usize].port_select;

        byte
    }
}



#[derive(Debug)]
pub struct Cartridge32k {
    pub data: Box<[u8; 0x8000]>
}




pub type FrameBuffer = Box<[u8; 128*128]>;
pub type SharedFrameBuffer = RefCell<FrameBuffer>;

pub fn new_framebuffer(fill: u8) -> SharedFrameBuffer {
    RefCell::new(Box::new([fill; 128*128]))
}
