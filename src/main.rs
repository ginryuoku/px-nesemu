use std::fs;
use std::io::Read;

struct Nes {
    cpu: Cpu,
    ram: [u8; 0x0800], // Famicom only has 2KiB of built-in RAM
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
        let cpu = Cpu { pc, a: 0, x: 0, y: 0, s: 0xFD, p: 0x34 };
        let ram = [0; 0x0800];

        Nes { cpu, ram, rom }
    }

    fn read_u8(&self, addr: u16) -> u8 {
        match addr {
            // RAM (mirrored every 0x0800 bytes)
            0x0000..=0x07FF => {
                let ram_offset = (addr as usize) % self.ram.len();
                self.ram[ram_offset]
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

    fn write_u8(&mut self, addr: u16, value: u8) {
        match addr {
            // RAM (mirrored every 0x0800 bytes)
            0x0000..=0x07FF => {
                let ram_offset = (addr as usize) % self.ram.len();
                self.ram[ram_offset] = value;
            }
            // PRG-ROM (writes are ignored for NROM)
            0x8000..=0xFFFF => { }
            _ => {
                unimplemented!("Write to ${:04X}", addr);
            }
        }
    }

    fn run(&mut self) {
        loop {
            let pc = self.cpu.pc;
            let opcode = self.read_u8(pc);

            let next_pc;
            match opcode {
                // LDA:
                //   Load immediate value into A
                0xA9 => {
                    let value =
                        self.read_u8(pc.wrapping_add(1));
                    self.cpu.a = value;
                    next_pc = pc.wrapping_add(2);
                }
                // ADC:
                //   Add immediate value to A
                0x69 => {
                    let value =
                        self.read_u8(pc.wrapping_add(1));
                    self.cpu.a =
                        self.cpu.a.wrapping_add(value);
                    next_pc = pc.wrapping_add(2);
                }
                // TAX:
                //   Transfer A to X
                0xAA => {
                    self.cpu.x = self.cpu.a;
                    next_pc = pc.wrapping_add(1);
                }
                // STX:
                //   Store X to address between
                //   $0000 and $00FF
                0x86 => {
                    let addr_lo =
                        self.read_u8(pc.wrapping_add(1));
                    // Between $0000 and $00FFF:
                    let addr = addr_lo as u16;
                    self.write_u8(addr, self.cpu.x);
                    next_pc = pc.wrapping_add(2);
                }
                // LDA:
                //   Load A from address between
                //   $0000 and $00FF
                0xA5 => {
                    let addr_lo =
                        self.read_u8(pc.wrapping_add(1));
                    // Between $0000 and $00FF:
                    let addr = addr_lo as u16;
                    self.cpu.a = self.read_u8(addr);
                    next_pc = pc.wrapping_add(2);
                }
                // JMP:
                //   Jump to address by changing PC
                0x4C => {
                    let target =
                        self.read_u16(pc.wrapping_add(1));
                    // Set PC to the address we just read:
                    next_pc = target;
                }
                _ => {
                    unimplemented!("Opcode {:02X}", opcode);
                }
            }

            self.cpu.pc = next_pc;

            // Some nice debug output so we can see
            // the CPU state after every cycle
            println!("Opcode: {:02X}", opcode);
            println!("CPU State: {:02X?}", self.cpu);
            println!("-----------------");
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
            .skip(10)
            .take(16_384)
            .collect::<Result<Vec<u8>, _>>()
            .unwrap();

        Rom { prg_rom }
    }
}

#[derive(Debug)]
struct Cpu {
    pc: u16,
    a: u8,
    x: u8,
    y: u8,
    s: u8,
    p: u8,
}

fn main() {
    let rom = Rom::from_file("tests/sample.nes");
    let mut nes = Nes::from_rom(rom);

    nes.run();
}
