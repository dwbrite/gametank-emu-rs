use crate::gametank_bus::Bus;

#[derive(Debug)]
pub struct Blitter {
    src_y: u8,
    dst_y: u8,
    height: u8,

    src_x: u8,
    dst_x: u8,
    width: u8,

    offset_x: u8,
    offset_y: u8,

    color_fill: bool,

    color: u8,
    blitting: bool,
    pub(crate) irq_trigger: bool,
}

impl Blitter {
    pub fn default() -> Self {
        Self {
            src_y: 0,
            dst_y: 0,
            height: 0,
            src_x: 0,
            dst_x: 0,
            width: 0,
            offset_x: 0,
            offset_y: 0,
            color_fill: false,
            color: 0,
            blitting: false,
            irq_trigger: false,
        }
    }

    pub fn clear_irq_trigger(&mut self) -> bool {
        let result = self.irq_trigger;
        self.irq_trigger = false;
        result
    }

    pub fn cycle(&mut self, bus: &mut Bus) {
        log::trace!(target: "blitter", "{:?}", self);

        // load y at blitter start
        if !self.blitting && bus.blitter.start != 0 {
            bus.blitter.start = 0;
            self.src_y = bus.blitter.gy;
            self.dst_y = bus.blitter.vy;
            self.height = bus.blitter.height;
            self.color = !bus.blitter.color;
            self.color_fill = bus.system_control.dma_flags.dma_colorfill_enable();
            self.blitting = true;

            log::trace!(target: "blitter", "starting {}x{} blit at ({}, {}); color mode {}", bus.blitter.width, bus.blitter.height, bus.blitter.vx, bus.blitter.vy, bus.system_control.dma_flags.dma_colorfill_enable());
        }

        if !self.blitting {
            return
        }

        self.src_x = bus.blitter.gx;
        self.dst_x = bus.blitter.vx;
        self.width = bus.blitter.width;
        //
        log::trace!(target: "blitter", "starting blit pixel, {}x{} at ({}, {})", self.width, self.height, self.dst_x, self.dst_y);

        if self.offset_x >= self.width {
            self.offset_x = 0;
            self.offset_y += 1;
        }

        if self.offset_y >= self.height {
            self.offset_y = 0;
            self.blitting = false;
            // log::trace!("blit complete");
            if bus.system_control.dma_flags.dma_irq() {
                self.irq_trigger = true;
            }
            return
        }

        // if blitter is disabled, counters continue but no write occurs
        if !bus.system_control.dma_flags.dma_enable() {
            log::trace!(target: "blitter", "blit cycle skipped; dma access disabled. dma flags: {:08b}", bus.system_control.dma_flags.0);
            self.offset_x += 1;
            return
        }

        // get the next color to write
        let color = if self.color_fill {
            self.color
        } else {
            let vram_page = bus.system_control.banking_register.vram_page() as usize;
            let quadrant = bus.blitter.vram_quadrant();

            let src_x_mod = if self.src_x >= 128 {
                self.src_x - 128
            } else {
                self.src_x
            };

            let src_y_mod = if self.src_y >= 128 {
                self.src_y - 128
            } else {
                self.src_y
            };


            let blit_src_x = (src_x_mod + self.offset_x) as usize;
            let blit_src_y = (src_y_mod + self.offset_y) as usize;
            log::trace!(target: "blitter", "starting blit pixel, {}x{} at ({}, {})", self.width, self.height, self.dst_x, self.dst_y);

            bus.vram_banks[vram_page][quadrant][blit_src_x + blit_src_y*128]
        };

        let out_x = (self.dst_x + self.offset_x) as usize;
        let out_y = (self.dst_y + self.offset_y) as usize;
        let out_fb = bus.system_control.banking_register.framebuffer() as usize;

        if out_x >= 128 || out_y >= 128 {
            self.offset_x +=1;
            return
        }

        // write to active framebuffer, if not transparent
        if bus.system_control.dma_flags.dma_opaque() || color != 0 {
            bus.framebuffers[out_fb].borrow_mut()[out_x + out_y*128] = color;
        }

        // increment x offset
        self.offset_x += 1;
    }
}
