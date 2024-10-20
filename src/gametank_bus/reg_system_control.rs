use tracing::warn;
use crate::gamepad::GamePad;
use crate::gametank_bus::reg_etc::{BankingRegister, BlitterFlags, GraphicsMemoryMap};

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
            0x2006 => { self.audio_enable_sample_rate = data }
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