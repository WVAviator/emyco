use std::fs;

use serde::{Deserialize, Serialize};

use crate::gameboy::memory::TestMemoryBus;

use super::{CPUFlags, CPU};

#[test]
fn cpu_json_instruction_tests() {
    let mut test_count = 0;
    for json_file in fs::read_dir("tests").unwrap() {
        let json_file_path = json_file.unwrap().path();
        let tests = read_json_file(json_file_path.to_str().unwrap());
        println!("Running tests from {:?}", json_file_path);
        for test in tests {
            test.run();
            test_count += 1;
        }
    }
    println!("Passed {} CPU tests.", test_count);
}

fn read_json_file(path: &str) -> Vec<JsonCpuTest> {
    let json_string = fs::read_to_string(path).unwrap();
    let data: Vec<JsonCpuTest> = serde_json::from_str(&json_string).unwrap();
    data
}

#[derive(Serialize, Deserialize, Debug)]
pub struct JsonCpuTest {
    name: String,

    #[serde(rename = "initial")]
    intial_state: CpuTestState,
    #[serde(rename = "final")]
    final_state: CpuTestState,
    cycles: Vec<[CpuTestCycleData; 3]>,
}

impl JsonCpuTest {
    pub fn run(&self) {
        let mut cpu = CPU::from(&self.intial_state);
        let mut m_cycles = 0;

        for [address, data, _] in self.cycles.iter() {
            cpu.tick(4);
            m_cycles += 1;
            if let (CpuTestCycleData::Address(address), CpuTestCycleData::Data(data)) =
                (address, data)
            {
                assert_eq!(
                    cpu.memory.read().unwrap().read_byte(*address),
                    *data,
                    "Cycle timing mismatch at m-cycle {}!",
                    m_cycles
                );
            }
        }

        let result = self.final_state.assert_matches(cpu);

        assert!(
            result.is_ok(),
            "{} | Final State Mismatch: {}",
            self.name,
            result.err().unwrap()
        )
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct CpuTestState {
    pc: u16,
    sp: u16,
    a: u8,
    b: u8,
    c: u8,
    d: u8,
    e: u8,
    f: u8,
    h: u8,
    l: u8,
    // ime: u8,
    // ei: u8,
    ram: Vec<[u16; 2]>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(untagged)]
pub enum CpuTestCycleData {
    Address(u16),
    Data(u8),
    Requests(String),
}

impl From<&CpuTestState> for CPU {
    fn from(value: &CpuTestState) -> Self {
        let memory = TestMemoryBus::new_shared();
        let mut cpu = CPU::new_test(memory.clone());

        cpu.program_counter = value.pc;
        cpu.stack_pointer = value.sp;
        cpu.register_a = value.a;
        cpu.register_b = value.b;
        cpu.register_c = value.c;
        cpu.register_d = value.d;
        cpu.register_e = value.e;
        cpu.register_f = CPUFlags::from_bits_truncate(value.f);
        cpu.register_h = value.h;
        cpu.register_l = value.l;

        // cpu.ime = match value.ime {
        //     0 => IMEState::Disabled,
        //     _ => IMEState::Enabled,
        // };

        // TODO: value.ei?

        let mut memory = memory.write().unwrap();

        for [address, byte] in value.ram.iter() {
            memory.write_byte(*address, *byte as u8);
        }

        cpu
    }
}

impl CpuTestState {
    fn assert_matches(&self, cpu: CPU) -> Result<(), String> {
        if self.pc != cpu.program_counter {
            return Err(format!(
                "Register PC | Expected: {} | Actual {}",
                self.pc, cpu.program_counter
            ));
        }

        if self.sp != cpu.stack_pointer {
            return Err(format!(
                "Register SP | Expected: {} | Actual {}",
                self.sp, cpu.stack_pointer
            ));
        }

        if self.a != cpu.register_a {
            return Err(format!(
                "Register A | Expected: {} | Actual {}",
                self.a, cpu.register_a
            ));
        }

        if self.b != cpu.register_b {
            return Err(format!(
                "Register B | Expected: {} | Actual {}",
                self.b, cpu.register_b
            ));
        }

        if self.c != cpu.register_c {
            return Err(format!(
                "Register C | Expected: {} | Actual {}",
                self.c, cpu.register_c
            ));
        }

        if self.d != cpu.register_d {
            return Err(format!(
                "Register D | Expected: {} | Actual {}",
                self.d, cpu.register_d
            ));
        }

        if self.e != cpu.register_e {
            return Err(format!(
                "Register E | Expected: {} | Actual {}",
                self.e, cpu.register_e
            ));
        }

        if self.f != cpu.register_f.bits() {
            return Err(format!(
                "Register F | Expected: {} | Actual {}",
                self.f,
                cpu.register_f.bits()
            ));
        }

        if self.h != cpu.register_h {
            return Err(format!(
                "Register H | Expected: {} | Actual {}",
                self.h, cpu.register_h
            ));
        }

        if self.l != cpu.register_l {
            return Err(format!(
                "Register L | Expected: {} | Actual {}",
                self.l, cpu.register_l
            ));
        }

        // TODO: IME and EI

        let memory = cpu.memory.read().unwrap();
        for [address, byte] in self.ram.iter() {
            let actual = memory.read_byte(*address);
            if actual != *byte as u8 {
                return Err(format!(
                    "Memory Addr {} | Expected {} | Actual {}",
                    address, byte, actual
                ));
            }
        }

        Ok(())
    }
}
