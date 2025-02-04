use std::{
    fmt::Debug,
    path::PathBuf,
    sync::{Arc, RwLock},
};

use anyhow::bail;
use log::info;

use std::{
    fs,
    time::{Duration, Instant},
};

use crossbeam::channel::Sender;
use log::error;

use super::{
    mbc::{BankType, NoMBC, MBC, MBC1, MBC2, MBC3},
    Register,
};

#[derive(Debug)]
pub struct Cartridge {
    mbc: Box<dyn MBC>,
    rom: Vec<u8>,
    ram: Arc<RwLock<Vec<u8>>>,
    persister: Option<Persister>,
    title: String,
}

impl Cartridge {
    pub fn new(rom: Vec<u8>, mut save_data_path: PathBuf) -> Result<Self, anyhow::Error> {
        let mbc_type = rom[0x147];
        let title: String = rom[0x0134..=0x0143]
            .iter()
            .take_while(|byte| **byte != 0x00)
            .map(|byte| *byte as char)
            .collect();

        save_data_path.push(format!("{}.sav", &title));
        info!("Preparing save data at location {:?}", save_data_path);

        let mbc: Box<dyn MBC> = match mbc_type {
            0x00 => Box::new(NoMBC::new()),
            0x01..=0x03 => {
                info!("Constructed cartridge with MBC1.");
                Box::new(MBC1::new())
            }
            0x05..=0x06 => {
                info!("Constructed cartridge with MBC2.");
                Box::new(MBC2::new())
            }
            0x0F..=0x13 => {
                info!("Constructed cartridge with MBC3.");
                Box::new(MBC3::new())
            }
            other => bail!("Unsupported MBC type {}.", other),
        };

        let ram_size = match rom[0x149] {
            0x00 => 0,
            0x01 => 2 * 1024,
            0x02 => 8 * 1024,
            0x03 => 32 * 1024,
            0x04 => 128 * 1024,
            0x05 => 64 * 1024,
            _ => bail!("Unsupported RAM size."),
        };

        let ram = vec![0; ram_size];
        let ram = Arc::new(RwLock::new(ram));

        let persister = match mbc_type {
            0x03 | 0x06 | 0x0F | 0x10 | 0x13 => Some(Persister::new(save_data_path, ram.clone())),
            _ => None,
        };

        info!(
            "Constructed cartridge with RAM size {} and ROM size {}.",
            ram.read().unwrap().len(),
            rom.len()
        );

        Ok(Cartridge {
            mbc,
            rom,
            ram,
            persister,
            title,
        })
    }

    pub fn get_title(&self) -> String {
        self.title.clone()
    }
}

impl Register for Cartridge {
    fn read(&self, address: u16) -> u8 {
        match self.mbc.translate_address(address) {
            Some((physical_address, BankType::ROM)) => {
                if self.rom.is_empty() {
                    return 0xFF;
                }
                self.rom[physical_address as usize % self.rom.len()]
            }
            Some((physical_address, BankType::RAM)) => {
                let ram_size = self.ram.read().unwrap().len();
                if ram_size == 0 {
                    return 0xFF;
                }
                self.ram.read().unwrap()[physical_address as usize % ram_size]
            }
            Some((_, BankType::RTC(value))) => value,
            _ => 0xFF,
        }
    }

    fn write(&mut self, address: u16, value: u8) {
        self.mbc.handle_control_write(address, value);

        if let Some((physical_address, BankType::RAM)) = self.mbc.translate_address(address) {
            let ram_size = self.ram.read().unwrap().len();

            if ram_size == 0 {
                return;
            }

            self.ram.write().unwrap()[physical_address as usize % ram_size] = value;

            if let Some(persister) = &mut self.persister {
                persister.write_data();
            }
        }
    }
}

const DEBOUNCE_TIME_SECS: u64 = 3;

#[derive(Debug)]
pub struct Persister {
    tx: Sender<PersistRequest>,
}

#[derive(Debug)]
struct PersistRequest;

impl Persister {
    pub fn new(path: PathBuf, ram: Arc<RwLock<Vec<u8>>>) -> Self {
        let (tx, rx) = crossbeam::channel::bounded(16);

        let existing_save_data = fs::read(&path).unwrap_or_default();

        if !existing_save_data.is_empty() {
            let mut ram = ram.write().unwrap();

            ram.resize(existing_save_data.len(), 0);
            ram.copy_from_slice(&existing_save_data);

            info!("Loaded existing game save from {:?}", &path);
        }

        std::thread::spawn(move || {
            let ram = ram.clone();

            loop {
                let _request = rx.recv().unwrap();
                let mut debounce_time = Instant::now() + Duration::from_secs(DEBOUNCE_TIME_SECS);

                while Instant::now() < debounce_time {
                    if let Ok(_request) = rx.try_recv() {
                        debounce_time = Instant::now() + Duration::from_secs(DEBOUNCE_TIME_SECS);
                    }
                }

                {
                    let ram = ram.read().unwrap().clone();

                    info!(
                        "Saving game data to {:?} following cartridge RAM write.",
                        &path
                    );
                    fs::write(&path, ram).expect("Unable to write to file.");
                }
            }
        });

        Persister { tx }
    }

    pub fn write_data(&mut self) {
        if let Err(e) = self.tx.try_send(PersistRequest) {
            error!("Unable to save data to file. {:?}", e);
        }
    }
}
