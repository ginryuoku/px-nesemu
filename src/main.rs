#![feature(generators, generator_trait)]

use std::fs;
use std::io::Read;
use std::pin::Pin;
use std::ops::{Generator, GeneratorState};
use std::cell::Cell;

struct Nes {
    cpu: Cpu,
    ram: Cell<[u8; 0x0800]>, // Famicom only has 2KiB of built-in RAM
    rom: Rom,
    // TODO: PPU
    // TODO: APU
}

impl Nes {
    fn from_rom(rom: Rom) -> Self {
        // Convert the 2 bytes at offset 0x3FFC / 0x3FFD
        // to a u16 to get PC
        // NOTE: This only works for NROM ROMs with
        // a size of 16 KiB!        
        let pc_bytes = &rom.prg_rom[0x3FFC..=0x3FFD];
        let pc = (pc_bytes[0] as u16) | ((pc_bytes[1] as u16) << 8);

        // See http://wiki.nesdev.com/w/index.php/CPU_power_up_state        
        let cpu = Cpu { 
            pc: Cell::new(pc),
            a: Cell::new(0), 
            x: Cell::new(0), 
            y: Cell::new(0), 
            s: Cell::new(0xFD), 
            p: Cell::new(0x34), 
            nmi: Cell::new(false),
        };
        let ram = Cell::new([0; 0x0800]);

        Nes { cpu, ram, rom }
    }

    fn read_u8(&self, addr: u16) -> u8 {
        match addr {
            // RAM (mirrored every 0x0800 bytes)
            0x0000..=0x07FF => {
                let ram: &Cell<[u8]> = &self.ram;
                let ram = ram.as_slice_of_cells();
                let ram_offset = (addr as usize) % ram.len();
                ram[ram_offset].get()
            }
            // PRG-ROM (mirrored to fill all 32 KiB)
            0x8000..=0xFFFF => {
                let rom_len = self.rom.prg_rom.len();
                let rom_offset = (addr as usize - 0x8000) % rom_len;
                self.rom.prg_rom[rom_offset]
            }
            _ => {
                unimplemented!("Read from ${:04X}", addr);
            }
        }
    }

    // This is the same logic we used in `Nes::from_rom`, so
    // we could refactor this
    fn read_u16(&self, addr: u16) -> u16 {
        let lo = self.read_u8(addr);
        let hi = self.read_u8(addr.wrapping_add(1));

        (lo as u16) | ((hi as u16) << 8)
    }

    fn write_u8(&self, addr: u16, value: u8) {
        match addr {
            // RAM (mirrored every 0x0800 bytes)
            0x0000..=0x07FF => {
                let ram: &Cell<[u8]> = &self.ram;
                let ram = ram.as_slice_of_cells();
                let ram_offset = (addr as usize) % ram.len();
                ram[ram_offset].set(value);
            }
            // PRG-ROM (writes are ignored for NROM)
            0x8000..=0xFFFF => { }
            _ => {
                unimplemented!("Write to ${:04X}", addr);
            }
        }
    }

    fn run_cpu<'a>(&'a self)
        -> impl Generator<Yield = (), Return = ()> + 'a
    {
        move || loop {
            if self.cpu.nmi.get() {
                // NOTE: It's intentional that this
                // check happens on the cycle before
                // the next instruction executes!

                // TODO: Read NMI interrupt vector
                // from address $FFFE, then set PC
                println!("=== NMI! ===");
                self.cpu.nmi.set(false);
            }

            let opcode = self.read_u8(self.cpu.pc.get());
            self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
            yield;

            match opcode {
                // LDA:
                //   Load immediate value into A
                0xA9 => {
                    let value = self.read_u8(self.cpu.pc.get());
                    self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
                    self.cpu.a.set(value);
                    yield;
                }
                // ADC:
                //   Add immediate value to A
                0x69 => {
                    let value = self.read_u8(self.cpu.pc.get());
                    self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
                    self.cpu.a.set(self.cpu.a.get().wrapping_add(value));
                    yield;
                }
                // TAX:
                //   Transfer A to X
                0xAA => {
                    let _garbage = self.read_u8(self.cpu.pc.get());
                    self.cpu.x.set(self.cpu.a.get());
                    yield;
                }
                // STX:
                //   Store X to address between
                //   $0000 and $00FF
                0x86 => {
                    // Cycle 2
                    let addr_lo =
                        self.read_u8(self.cpu.pc.get());
                    // Between $0000 and $00FFF:
                    let addr = addr_lo as u16;
                    self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
                    yield;

                    // Cycle 3
                    self.write_u8(addr, self.cpu.x.get());
                    yield;
                }
                // LDA:
                //   Load A from address between
                //   $0000 and $00FF
                0xA5 => {
                    // cycle 2 (read)
                    let addr_lo = self.read_u8(self.cpu.pc.get());
                    // Between $0000 and $00FF:
                    let addr = addr_lo as u16;
                    self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
                    yield;
                    // cycle 3 (modify)
                    let value = self.read_u8(addr);
                    self.cpu.a.set(value);
                    yield;
                }
                // JMP:
                //   Jump to address by changing PC
                0x4C => {
                    // Cycle 2:
                    //   Read the low address of the jump
                    //   target by reading PC, then increment PC
                    let target_lo = self.read_u8(self.cpu.pc.get());
                    self.cpu.pc.set(self.cpu.pc.get().wrapping_add(1));
                    yield;
                    
                    // Cycle 3:
                    //   Read the high address of the jump
                    //   target and set PC
                    let target_hi = self.read_u8(self.cpu.pc.get());
                    let target =
                        (target_lo as u16)
                        | ((target_hi as u16) << 8);
                    self.cpu.pc.set(target);
                    yield;
                }
                _ => {
                    unimplemented!("Opcode {:02X}", opcode);
                }
            }

            // Some nice debug output so we can see
            // the CPU state after every cycle
            println!("Opcode: {:02X}", opcode);
            println!("CPU State: {:02X?}", self.cpu);
            println!("-----------------");
        }
    }    
    
    fn run<'a>(&'a self) -> impl Generator<Yield = (), Return = ()> + 'a {
        let mut run_cpu = self.run_cpu();
        let mut run_ppu = self.run_ppu();

        move || loop {
            match Pin::new(&mut run_cpu).resume() {
                GeneratorState::Yielded(()) => { }
                GeneratorState::Complete(_) => { break; }
            }

            // step PPU for 3 cycles (PPU is 3x faster than CPU)
            for _ in 0..3 {
                match Pin::new(&mut run_ppu).resume() {
                    GeneratorState::Yielded(()) => { }
                    GeneratorState::Complete(_) => { break; }
                }
            }

            // yield one cycle - both CPU and PPU have run
            yield;
        }
    }

    fn run_ppu<'a>(&'a self) -> impl Generator<Yield = (), Return = ()> + 'a {
        move || loop {
            for _frame in 0.. {
                // - Each PPU cycle produces 1 pixel
                // - Each line lasts 341 cycles (256 visible)
                // - Each frame lasts 262 lines (240 visible)
                const PPU_CYCLES_PER_FRAME: u32 = 341 * 262;
                for cycle in 0..PPU_CYCLES_PER_FRAME {
                    // NMI starts at the *second* cycle!
                    if cycle == 1 {
                        self.cpu.nmi.set(true);
                    }

                    // TODO: Output pixels

                    yield;
                }
            }
        }
    }    
}

struct Rom {
    prg_rom: Vec<u8>, // we're only doing no-mapper ROMs, so we only need PRG-ROM
}

impl Rom {
    fn from_file(filename: &str) -> Rom {
        let rom_file = fs::File::open(filename).unwrap();

        // Skip the first 10 bytes, read 16 KiB for our PRG-ROM
        // TODO: Actually parse the ROM header!
        let prg_rom: Vec<u8> = rom_file
            .bytes()
            .skip(16)
            .take(16_384)
            .collect::<Result<Vec<u8>, _>>()
            .unwrap();

        Rom { prg_rom }
    }
}

#[derive(Debug)]
struct Cpu {
    pc: Cell<u16>,
    a: Cell<u8>,
    x: Cell<u8>,
    y: Cell<u8>,
    s: Cell<u8>,
    p: Cell<u8>,
    nmi: Cell<bool>,
}

fn main() {
    //let rom = sample_rom();
    let rom = Rom::from_file("tests/sample.nes");
    let nes = Nes::from_rom(rom);

    let mut nes_run = nes.run();
    loop {
        match Pin::new(&mut nes_run).resume() {
            GeneratorState::Yielded(()) => {
                println!("> Cycle");
            }
            GeneratorState::Complete(_) => {
                // stop running if our run generator stops
                break;
            }
        }
    }
}

fn sample_rom() -> Rom {
    let interrupt_vectors = vec![0x00, 0x00, 0x00, 0x80, 0x00, 0x00];
    let program = vec![
        0xA9, 0x05,
        0x69, 0x06,
        0xAA,
        0x86, 0x01,
        0xA5, 0x01,
        0x4C, 0x09, 0x80,
    ];
    let prg_rom: Vec<u8> = program.into_iter()  // Program bytes
        .chain(std::iter::repeat(0))            // ...padded with zeros
        .take(0x4000 - interrupt_vectors.len()) // ...to fill 16 KiB - 6 bytes
        .chain(interrupt_vectors)               // ...followed by interrupt vectors
        .collect();                             // ...put into a vector of bytes
        
    // This is equivalent to loading our sample.nes file!
    Rom { prg_rom }
}
