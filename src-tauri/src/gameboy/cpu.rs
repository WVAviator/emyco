#[cfg(test)]
mod test;

use arraydeque::{ArrayDeque, Saturating};
use bitflags::bitflags;
use log::trace;
use std::ops::{Deref, DerefMut};

use super::memory::SharedMemoryController;

const OP_QUEUE_SIZE: usize = 16;

#[allow(clippy::upper_case_acronyms)]
pub struct CPU {
    register_a: u8,
    register_b: u8,
    register_c: u8,
    register_d: u8,
    register_e: u8,
    register_f: CPUFlags,
    register_h: u8,
    register_l: u8,
    stack_pointer: u16,
    program_counter: u16,
    memory: SharedMemoryController,
    ime: IMEState,
    state: CPUState,
    pending_cycles: i32,
    interrupt_dispatch: InterruptDispatchState,
    operation_queue: OperationQueue<Operation, OP_QUEUE_SIZE>,
    internal_buffer: Vec<u8>,
}

impl CPU {
    #[allow(dead_code)]
    pub fn new_test(memory: SharedMemoryController) -> Self {
        CPU::new(memory)
    }

    pub fn new(memory: SharedMemoryController) -> Self {
        CPU {
            register_a: 0,
            register_b: 0,
            register_c: 0,
            register_d: 0,
            register_e: 0,
            register_f: CPUFlags::empty(),
            register_h: 0,
            register_l: 0,
            stack_pointer: 0xFFFE,
            program_counter: 0x0100,
            memory,
            ime: IMEState::Disabled,
            state: CPUState::Ready,
            pending_cycles: 0,
            interrupt_dispatch: InterruptDispatchState::Waiting,
            operation_queue: OperationQueue::new(),
            internal_buffer: Vec::new(),
        }
    }

    #[inline]
    fn read_target(&mut self, target: &OpTarget) -> u8 {
        match target {
            OpTarget::RegisterA => self.register_a,
            OpTarget::RegisterB => self.register_b,
            OpTarget::RegisterC => self.register_c,
            OpTarget::RegisterD => self.register_d,
            OpTarget::RegisterE => self.register_e,
            OpTarget::RegisterF => self.register_f.bits(),
            OpTarget::RegisterH => self.register_h,
            OpTarget::RegisterL => self.register_l,
            OpTarget::ProgramCounterLow => (self.program_counter & 0xFF) as u8,
            OpTarget::ProgramCounterHigh => ((self.program_counter & 0xFF00) >> 8) as u8,
            OpTarget::StackPointerLow => (self.stack_pointer & 0xFF) as u8,
            OpTarget::StackPointerHigh => ((self.stack_pointer & 0xFF00) >> 8) as u8,
            OpTarget::MemoryAddress(address) => self.read_memory(*address),
            OpTarget::Immediate(value) => *value,
            OpTarget::InternalBuffer => self
                .internal_buffer
                .pop()
                .expect("Tried to read from empty internal buffer."),
            OpTarget::None => 0,
        }
    }

    #[inline]
    fn write_target(&mut self, target: OpTarget, value: u8) {
        match target {
            OpTarget::RegisterA => self.register_a = value,
            OpTarget::RegisterB => self.register_b = value,
            OpTarget::RegisterC => self.register_c = value,
            OpTarget::RegisterD => self.register_d = value,
            OpTarget::RegisterE => self.register_e = value,
            OpTarget::RegisterF => self.register_f = CPUFlags::from_bits_truncate(value),
            OpTarget::RegisterH => self.register_h = value,
            OpTarget::RegisterL => self.register_l = value,
            OpTarget::ProgramCounterLow => {
                self.program_counter = (self.program_counter & 0xFF00) | (value as u16)
            }
            OpTarget::ProgramCounterHigh => {
                self.program_counter = (self.program_counter & 0x00FF) | ((value as u16) << 8)
            }
            OpTarget::StackPointerLow => {
                self.stack_pointer = (self.stack_pointer & 0xFF00) | (value as u16)
            }
            OpTarget::StackPointerHigh => {
                self.stack_pointer = (self.stack_pointer & 0x00FF) | ((value as u16) << 8)
            }
            OpTarget::InternalBuffer => {
                self.internal_buffer.push(value);
            }
            OpTarget::Immediate(_) => {}
            OpTarget::MemoryAddress(address) => self.write_memory(address, value),
            OpTarget::None => {}
        }
    }

    #[inline]
    fn perform_arithmetic(&mut self, operation: ArithmeticOperation, lhs: OpTarget, rhs: OpTarget) {
        let lhs_value = self.read_target(&lhs);
        let rhs_value = self.read_target(&rhs);

        match operation {
            ArithmeticOperation::Increment => {
                let new_value = lhs_value.wrapping_add(1);

                self.register_f.set(CPUFlags::ZERO, new_value == 0);
                self.register_f
                    .set(CPUFlags::HALF_CARRY, check_half_carry_add(lhs_value, 1));
                self.register_f.remove(CPUFlags::SUBTRACT);

                self.write_target(lhs, new_value);
            }
            ArithmeticOperation::Decrement => {
                let new_value = lhs_value.wrapping_sub(1);

                self.register_f.set(CPUFlags::ZERO, new_value == 0);
                self.register_f
                    .set(CPUFlags::HALF_CARRY, check_half_carry_sub(lhs_value, 1));
                self.register_f.insert(CPUFlags::SUBTRACT);

                self.write_target(lhs, new_value);
            }
            ArithmeticOperation::RotateLeft {
                set_zero,
                through_carry,
            } => match through_carry {
                true => {
                    let carry = self.register_f.contains(CPUFlags::CARRY);
                    self.register_f.set(CPUFlags::CARRY, lhs_value & 0x80 != 0);
                    let mut new_value = lhs_value << 1;
                    if carry {
                        new_value |= 1;
                    }

                    self.register_f
                        .set(CPUFlags::ZERO, new_value == 0 && set_zero);
                    self.register_f
                        .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                    self.write_target(lhs, new_value);
                }
                false => {
                    self.register_f.set(CPUFlags::CARRY, lhs_value & 0x80 != 0);
                    let new_value = lhs_value.rotate_left(1);

                    self.register_f
                        .set(CPUFlags::ZERO, new_value == 0 && set_zero);
                    self.register_f
                        .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                    self.write_target(lhs, new_value);
                }
            },
            ArithmeticOperation::RotateRight {
                set_zero,
                through_carry,
            } => match through_carry {
                true => {
                    let carry = self.register_f.contains(CPUFlags::CARRY);
                    self.register_f.set(CPUFlags::CARRY, lhs_value & 0x01 != 0);
                    let new_value = (lhs_value >> 1) | ((carry as u8) << 7);

                    self.register_f
                        .set(CPUFlags::ZERO, new_value == 0 && set_zero);
                    self.register_f
                        .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                    self.write_target(lhs, new_value);
                }
                false => {
                    self.register_f.set(CPUFlags::CARRY, lhs_value & 0x01 != 0);
                    let new_value = lhs_value.rotate_right(1);

                    self.register_f
                        .set(CPUFlags::ZERO, new_value == 0 && set_zero);
                    self.register_f
                        .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                    self.write_target(lhs, new_value);
                }
            },
            ArithmeticOperation::Add => {
                let half_carry = check_half_carry_add(lhs_value, rhs_value);
                let (result, carry) = lhs_value.overflowing_add(rhs_value);

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.set(CPUFlags::HALF_CARRY, half_carry);
                self.register_f.set(CPUFlags::CARRY, carry);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Sub => {
                let half_carry = check_half_carry_sub(lhs_value, rhs_value);
                let (result, carry) = lhs_value.overflowing_sub(rhs_value);

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.insert(CPUFlags::SUBTRACT);
                self.register_f.set(CPUFlags::HALF_CARRY, half_carry);
                self.register_f.set(CPUFlags::CARRY, carry);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::AddWithCarry => {
                let has_carry = self.register_f.contains(CPUFlags::CARRY);
                let half_carry = check_half_carry_adc(lhs_value, rhs_value, has_carry);
                let (result1, carry1) = lhs_value.overflowing_add(rhs_value);
                let (result2, carry2) = result1.overflowing_add(has_carry as u8);

                self.register_f.set(CPUFlags::ZERO, result2 == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.set(CPUFlags::HALF_CARRY, half_carry);
                self.register_f.set(CPUFlags::CARRY, carry1 || carry2);

                self.write_target(lhs, result2);
            }
            ArithmeticOperation::SubWithCarry => {
                let has_carry = self.register_f.contains(CPUFlags::CARRY);
                let half_carry = check_half_carry_sbc(lhs_value, rhs_value, has_carry);
                let (result1, carry1) = lhs_value.overflowing_sub(rhs_value);
                let (result2, carry2) = result1.overflowing_sub(has_carry as u8);

                self.register_f.set(CPUFlags::ZERO, result2 == 0);
                self.register_f.insert(CPUFlags::SUBTRACT);
                self.register_f.set(CPUFlags::HALF_CARRY, half_carry);
                self.register_f.set(CPUFlags::CARRY, carry1 || carry2);

                self.write_target(lhs, result2);
            }
            ArithmeticOperation::And => {
                let result = lhs_value & rhs_value;

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.insert(CPUFlags::HALF_CARRY);
                self.register_f.remove(CPUFlags::CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Xor => {
                let result = lhs_value ^ rhs_value;

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.remove(CPUFlags::HALF_CARRY);
                self.register_f.remove(CPUFlags::CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Or => {
                let result = lhs_value | rhs_value;

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.remove(CPUFlags::HALF_CARRY);
                self.register_f.remove(CPUFlags::CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Compare => {
                let half_carry = check_half_carry_sub(lhs_value, rhs_value);
                let (result, carry) = lhs_value.overflowing_sub(rhs_value);

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f.insert(CPUFlags::SUBTRACT);
                self.register_f.set(CPUFlags::HALF_CARRY, half_carry);
                self.register_f.set(CPUFlags::CARRY, carry);
            }
            ArithmeticOperation::ShiftLeft => {
                self.register_f
                    .set(CPUFlags::CARRY, lhs_value & 0b1000_0000 != 0);

                let result = lhs_value << 1;

                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f
                    .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::ShiftRight {
                arithmetically: true,
            } => {
                self.register_f
                    .set(CPUFlags::CARRY, lhs_value & 0b0000_0001 != 0);

                let result = (lhs_value & 0b1000_0000) | (lhs_value >> 1);
                self.register_f.set(CPUFlags::ZERO, result == 0);
                self.register_f
                    .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::ShiftRight {
                arithmetically: false,
            } => {
                self.register_f
                    .set(CPUFlags::CARRY, lhs_value & 0b0000_0001 != 0);

                let result = lhs_value >> 1;

                self.register_f.set(CPUFlags::ZERO, result == 0);

                self.register_f
                    .remove(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Swap => {
                let result = lhs_value.rotate_left(4);
                self.register_f = CPUFlags::empty();
                self.register_f.set(CPUFlags::ZERO, result == 0);

                self.write_target(lhs, result);
            }
            ArithmeticOperation::Bit(bit) => {
                self.register_f
                    .set(CPUFlags::ZERO, lhs_value & (1 << bit) == 0);
                self.register_f.remove(CPUFlags::SUBTRACT);
                self.register_f.insert(CPUFlags::HALF_CARRY);
            }
            ArithmeticOperation::Res(bit) => {
                let result = lhs_value & !(1 << bit);
                self.write_target(lhs, result);
            }
            ArithmeticOperation::Set(bit) => {
                let result = lhs_value | (1 << bit);
                self.write_target(lhs, result);
            }
        }
    }

    pub fn tick(&mut self, cycles: u32) {
        self.pending_cycles += cycles as i32;
        while self.pending_cycles >= 4 {
            self.pending_cycles -= 4;

            self.handle_interrupts();

            if self.state == CPUState::Halted {
                continue;
            }

            if self.operation_queue.is_empty() {
                self.load_operation();

                if self.ime == IMEState::WillEnable {
                    self.ime = IMEState::Enabled;
                }
            }

            match self.operation_queue.pop_front().unwrap() {
                Operation::Nop | Operation::Internal | Operation::Fetch => {}
                Operation::Parallel(n) => {
                    // Allows executing the next n operations in parallel.
                    // Adds n * 4 cycles inside while loop to run more operations back to back.
                    self.pending_cycles += n as i32 * 4;
                }
                Operation::ReadWrite {
                    read_from,
                    write_to,
                } => {
                    let value = self.read_target(&read_from);
                    self.write_target(write_to, value);
                }
                Operation::ReadWriteWithFlags {
                    read_from,
                    write_to,
                    flags,
                } => {
                    let value = self.read_target(&read_from);
                    self.write_target(write_to, value);
                    self.register_f = flags;
                }
                Operation::Arithmetic {
                    lhs,
                    operation,
                    rhs,
                } => {
                    self.perform_arithmetic(operation, lhs, rhs);
                }
                Operation::StackPush(read_from) => {
                    let value = self.read_target(&read_from);
                    self.stack_pointer = self.stack_pointer.wrapping_sub(1);
                    self.write_memory(self.stack_pointer, value);
                }
                Operation::StackPop(write_to) => {
                    let value = self.read_memory(self.stack_pointer);
                    self.stack_pointer = self.stack_pointer.wrapping_add(1);
                    self.write_target(write_to, value);
                }
                Operation::InterruptDispatch => {
                    self.ime = IMEState::Disabled;

                    match self.interrupt_dispatch {
                        InterruptDispatchState::Cancelling => {
                            // IE flag was unset during interrupt handling. Program counter gets set to
                            // 0x0000.
                            trace!("Interrupt cancelled due to IE register change before PC set.");
                            self.program_counter = 0x0000;
                        }
                        InterruptDispatchState::Finalizing {
                            interrupt_enable,
                            interrupt_flag,
                        } => {
                            let interrupt = (interrupt_enable & interrupt_flag).trailing_zeros();

                            // Clear IF flag to mark interrupt as handled
                            let interrupt_flag = interrupt_flag & !(1 << interrupt);
                            self.write_memory(0xFF0F, interrupt_flag);

                            let interrupt_vector = match interrupt {
                                0 => 0x0040, // V-Blank
                                1 => 0x0048, // LCD STAT
                                2 => 0x0050, // Timer
                                3 => 0x0058, // Serial
                                4 => 0x0060, // Joypad
                                _ => return,
                            };

                            trace!("Jumping to interrupt vector {:#05x}", interrupt_vector);
                            self.program_counter = interrupt_vector;
                        }
                        _ => {
                            panic!(
                                "Encountered invalid interrupt state. Interrupt dispatch operation was requested but there are no interrupts to dispatch."
                            );
                        }
                    }

                    self.interrupt_dispatch = InterruptDispatchState::Waiting;
                }
            }
        }
    }

    #[inline]
    fn handle_interrupts(&mut self) {
        match self.interrupt_dispatch {
            InterruptDispatchState::Waiting => {
                if !self.operation_queue.is_empty() {
                    // Interrupts are only handled between instructions.
                    return;
                }

                let interrupt_enable = self.read_memory(0xFFFF);
                let interrupt_flag = self.read_memory(0xFF0F);

                if interrupt_enable & interrupt_flag == 0 {
                    return;
                }

                match self.ime {
                    IMEState::Enabled => {
                        trace!("Initiating interrupt routine.");

                        let mut dispatch_operations = 2;

                        if self.state == CPUState::Halted {
                            self.operation_queue.push_back(Operation::Nop);
                            dispatch_operations += 1;
                            self.state = CPUState::Ready;
                        }

                        self.interrupt_dispatch = InterruptDispatchState::Dispatching {
                            operations_remaining: dispatch_operations,
                        };

                        self.operation_queue.extend([
                            Operation::Nop,
                            Operation::Nop,
                            Operation::StackPush(OpTarget::ProgramCounterHigh),
                            Operation::StackPush(OpTarget::ProgramCounterLow),
                            Operation::InterruptDispatch,
                        ]);
                    }
                    IMEState::Disabled | IMEState::WillEnable => {
                        if self.state == CPUState::Halted {
                            self.operation_queue.push_back(Operation::Nop);
                            self.state = CPUState::Ready;
                        }
                    }
                }
            }
            InterruptDispatchState::Dispatching {
                operations_remaining: 0,
            } => {
                let interrupt_enable = self.read_memory(0xFFFF);
                let interrupt_flag = self.read_memory(0xFF0F);

                if interrupt_enable & interrupt_flag == 0 {
                    self.interrupt_dispatch = InterruptDispatchState::Cancelling;
                    return;
                }

                self.interrupt_dispatch = InterruptDispatchState::Finalizing {
                    interrupt_enable,
                    interrupt_flag,
                };
            }
            InterruptDispatchState::Dispatching {
                operations_remaining,
            } => {
                let interrupt_enable = self.read_memory(0xFFFF);
                let interrupt_flag = self.read_memory(0xFF0F);

                if interrupt_enable & interrupt_flag == 0 {
                    self.interrupt_dispatch = InterruptDispatchState::Cancelling;
                    return;
                }

                self.interrupt_dispatch = InterruptDispatchState::Dispatching {
                    operations_remaining: operations_remaining - 1,
                };
            }
            _ => {}
        }
    }

    pub fn reboot(&mut self) {
        self.program_counter = 0;
    }

    fn load_operation(&mut self) {
        let next_instruction = match self.state {
            CPUState::HaltBug => {
                self.state = CPUState::Ready;
                self.read_memory(self.program_counter)
            }
            _ => self.read_next_pc(),
        };

        trace!(
            "CPU: {:#05x} | {:#03x} .. {:#03x} .. {:#03x}",
            self.program_counter - 1,
            next_instruction,
            self.read_memory(self.program_counter),
            self.read_memory(self.program_counter + 1),
        );

        use ArithmeticOperation::*;
        use OpTarget::*;
        use Operation::*;

        match next_instruction {
            // NOP | 1 4 | - - - -
            // No operation
            0x00 => {
                self.operation_queue.push_back(Nop);
            }

            // LD BC, n16 | 3 12 | - - - -
            // Copy the value n16 into register BC
            0x01 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(low),
                    write_to: RegisterC,
                });
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(high),
                    write_to: RegisterB,
                });
            }

            // LD [BC], A | 1 8 | - - - -
            // Copy the value in register A into the byte pointed to by BC
            0x02 => {
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: RegisterA,
                    write_to: MemoryAddress(self.get_bc()),
                });
            }

            // INC BC | 1 8 | - - - -
            // Increment the value in register BC by 1
            0x03 => {
                let value = self.get_bc().wrapping_add(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterC,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterB,
                    },
                ]);
            }

            // INC B | 1 4 | Z 0 H -
            // Increment the value in register B by 1
            0x04 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterB,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC B | 1 4 | Z 1 H -
            // Decrement the value in register B by 1
            0x05 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterB,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD B, n8 | 2 8 | - - - -
            // Copy the value n8 into register B
            0x06 => {
                let value = self.read_next_pc();
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(value),
                    write_to: RegisterB,
                });
            }

            // RLCA | 1 4 | 0 0 0 C
            // Rotate register A left. 7th bit is pushed to the carry flag and also rotated to bit
            // 0.
            0x07 => {
                self.operation_queue.push_back(Arithmetic {
                    operation: RotateLeft {
                        set_zero: false,
                        through_carry: false,
                    },
                    lhs: RegisterA,
                    rhs: None,
                });
            }

            // LD [a16], SP | 3 20 | - - - -
            // Copy SP & $FF at address a16 and SP >> 8 at address a16 + 1.
            0x08 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();
                let address = (low as u16) | ((high as u16) << 8);

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: StackPointerLow,
                    write_to: MemoryAddress(address),
                });
                self.operation_queue.push_back(ReadWrite {
                    read_from: StackPointerHigh,
                    write_to: MemoryAddress(address + 1),
                });
            }

            // ADD HL, BC | 1 8 | - 0 H C
            // Add the value in BC to HL.
            // H - Set if overflow from bit 11
            // C - Set if overflow from bit 15
            0x09 => {
                let half_carry = check_half_carry_add_u16(self.get_hl(), self.get_bc());

                let (result, carry) = self.get_hl().overflowing_add(self.get_bc());

                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate((result & 0xFF) as u8),
                    write_to: RegisterL,
                });

                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(((result & 0xFF00) >> 8) as u8),
                    write_to: RegisterH,
                    flags,
                });
            }

            // LD A, [BC] | 1 8 | - - - -
            // Copy the byte pointed to by BC into register A.
            0x0A => {
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: MemoryAddress(self.get_bc()),
                    write_to: RegisterA,
                });
            }

            // DEC BC | 1 8 | - - - -
            // Decrement the value in register BC by 1.
            0x0B => {
                let value = self.get_bc().wrapping_sub(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterC,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterB,
                    },
                ]);
            }

            // INC C | 1 4 | Z 0 H -
            // Increment the value in register C by 1
            0x0C => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterC,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC C | 1 4 | Z 1 H -
            // Decrement the value in register C by 1
            0x0D => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterC,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD C, n8 | 2 8 | - - - -
            // Copy the value n8 into register C.
            0x0E => {
                let value = self.read_next_pc();

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(value),
                    write_to: RegisterC,
                });
            }

            // RRCA | 1 4 | 0 0 0 C
            // Rotate register A right. The 0th bit is pushed to the carry flag and also
            // rotated to bit 7.
            0x0F => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation: RotateRight {
                        set_zero: false,
                        through_carry: false,
                    },
                    rhs: None,
                });
            }

            // STOP n8 | 2 4 | - - - -
            0x10 => {
                let button_held = self.read_memory(0xFF00) & 0x0F != 0;
                let interrupt_pending = self.read_memory(0xFFFF) == self.read_memory(0xFF0F);

                // TODO: CGB implementation will require this be reworked

                match (button_held, interrupt_pending) {
                    (true, true) => {
                        // STOP is a one-byte op code, mode doesn't change, DIV is not reset
                        self.operation_queue.push_back(Nop);
                    }
                    (true, false) => {
                        // STOP is a two-byte opcode, HALT is eneter, DIV not reset
                        self.operation_queue.extend([Nop, Nop]);
                        self.state = CPUState::Halted;
                    }
                    (false, true) => {
                        // STOP is one-byte opcode, STOP mode is entered, DIV is reset

                        self.operation_queue.push_back(Nop);
                        self.write_memory(0xFF04, 0x01);
                        self.state = CPUState::Halted;
                    }
                    (false, false) => {
                        // STOP is a two-byte opcode, STOP mode is entered, DIV is reset

                        self.operation_queue.extend([Nop, Nop]);
                        self.write_memory(0xFF04, 0x01);
                        self.state = CPUState::Halted;
                    }
                }

                self.state = CPUState::Halted;
                self.operation_queue.push_back(Nop);
            }

            // LD DE, n16 | 3 12 | - - - -
            // Copy the value n16 into register DE
            0x11 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(low),
                    write_to: RegisterE,
                });
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(high),
                    write_to: RegisterD,
                });
            }

            // LD [DE], A | 1 8 | - - - -
            // Copy the value in register A into the byte pointed to by DE
            0x12 => {
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: RegisterA,
                    write_to: MemoryAddress(self.get_de()),
                });
            }

            // INC DE | 1 8 | - - - -
            // Increment register DE by 1.
            0x13 => {
                let value = self.get_de().wrapping_add(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterE,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterD,
                    },
                ]);
            }

            // INC D | 1 4 | Z 0 H -
            // Increment the value in register D by 1.
            0x14 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterD,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC D | 1 4 | Z 1 H -
            // Decrement the value in register D by 1.
            0x15 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterD,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD D, n8 | 2 8 | - - - -
            // Copy the value n8 into register D
            0x16 => {
                let value = self.read_next_pc();
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(value),
                    write_to: RegisterD,
                });
            }

            // RLA | 1 4 | 0 0 0 C
            // Rotate register A left, through the carry flag.
            0x17 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation: RotateLeft {
                        set_zero: false,
                        through_carry: true,
                    },
                    rhs: None,
                });
            }

            // JR e8 | 2 12 | - - - -
            // Relative jump by signed 8-bit offset
            0x18 => {
                let offset = self.read_next_pc() as i8;
                let destination = self.program_counter.wrapping_add(offset as i16 as u16);

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(destination.low()),
                    write_to: ProgramCounterLow,
                });
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(destination.high()),
                    write_to: ProgramCounterHigh,
                });
            }

            // ADD HL, DE | 1 8 | - 0 H C
            // Add the value in DE to HL.
            // H set if overflow from bit 11
            // C set if overflow from bit 15
            0x19 => {
                let half_carry = check_half_carry_add_u16(self.get_hl(), self.get_de());

                let (result, carry) = self.get_hl().overflowing_add(self.get_de());

                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate((result & 0xFF) as u8),
                    write_to: RegisterL,
                });

                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(((result & 0xFF00) >> 8) as u8),
                    write_to: RegisterH,
                    flags,
                });
            }

            // LD A, [DE] | 1 8 | - - - -
            // Copy the byte pointed to by DE into register A.
            0x1A => {
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: MemoryAddress(self.get_de()),
                    write_to: RegisterA,
                });
            }

            // DEC DE | 1 8 | - - - -
            // Decrement register DE by 1.
            0x1B => {
                let value = self.get_de().wrapping_sub(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterE,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterD,
                    },
                ]);
            }

            // INC E | 1 4 | Z 0 H -
            // Increment the value in register E by 1.
            0x1C => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterE,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC E | 1 4 | Z 1 H -
            // Decrement the value in register E by 1.
            0x1D => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterE,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD E, n8 | 2 8 | - - - -
            // Load the value n8 into register E.
            0x1E => {
                let value = self.read_next_pc();
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(value),
                    write_to: RegisterE,
                });
            }

            // RRA | 1 4 | 0 0 0 C
            // Rotate register A right, through the carry flag.
            0x1F => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation: RotateRight {
                        set_zero: false,
                        through_carry: true,
                    },
                    rhs: None,
                });
            }

            // JR NZ, e8 | 2 12/8 | - - - -
            // Relative Jump to address n16 if condition cc is met.
            // NZ - Execute if Z not set
            // Takes 1 extra cycle if condition met
            0x20 => {
                let offset = self.read_next_pc() as i8;

                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        let value = self.program_counter.wrapping_add(offset as i16 as u16);
                        self.operation_queue.push_back(ReadWrite {
                            read_from: Immediate(value.low()),
                            write_to: ProgramCounterLow,
                        });
                        self.operation_queue.push_back(ReadWrite {
                            read_from: Immediate(value.high()),
                            write_to: ProgramCounterHigh,
                        });
                    }
                }
            }

            // LD HL, n16 | 3 12 | - - - -
            // Copy the value n16 into register HL.
            0x21 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(low),
                    write_to: RegisterL,
                });
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(high),
                    write_to: RegisterH,
                });
            }

            // LD [HL+], A | 1 8 | - - - -
            // Copy the value in register A into the byte pointed by HL and increment HL afterwards.
            0x22 => {
                let address = self.get_hl();
                let value = self.get_hl().wrapping_add(1);
                self.operation_queue.extend([
                    Fetch,
                    Parallel(3),
                    ReadWrite {
                        read_from: RegisterA,
                        write_to: MemoryAddress(address),
                    },
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // INC HL | 1 8 | - - - -
            // Increment the value in register HL
            0x23 => {
                let value = self.get_hl().wrapping_add(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // INC H | 1 4 | Z 0 H -
            // Increment the value in register H by 1.
            0x24 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterH,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC H | 1 4 | Z 1 H -
            // Decrement the value in register H by 1.
            0x25 => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterH,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD H, n8 | 2 8 | - - - -
            // Load the value n8 into register H
            0x26 => {
                let value = self.read_next_pc();
                self.operation_queue.push_back(Fetch);
                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(value),
                    write_to: RegisterH,
                });
            }

            // DAA | 1 4 | Z - 0 C
            // Decimal Adjust Accumulator
            // Designed to be used after performing an arithmetic instruction (ADD, ADC, SUB, SBC)
            // whose inputs were in Binary-Coded Decimal (BCD), adjusting the result to likewise be in BCD.
            0x27 => {
                let mut flags = self.register_f;

                let result = match self.register_f.contains(CPUFlags::SUBTRACT) {
                    true => {
                        let mut adjustment = 0;
                        if self.register_f.contains(CPUFlags::HALF_CARRY) {
                            adjustment += 0x06;
                        }

                        if self.register_f.contains(CPUFlags::CARRY) {
                            adjustment += 0x60;
                            flags.insert(CPUFlags::CARRY);
                        }

                        self.register_a.wrapping_sub(adjustment)
                    }
                    false => {
                        let mut adjustment = 0;
                        if self.register_f.contains(CPUFlags::HALF_CARRY)
                            || (self.register_a & 0x0F) > 0x09
                        {
                            adjustment += 0x06;
                        }
                        if self.register_f.contains(CPUFlags::CARRY) || self.register_a > 0x99 {
                            adjustment += 0x60;
                            flags.insert(CPUFlags::CARRY);
                        }
                        self.register_a.wrapping_add(adjustment)
                    }
                };

                flags.set(CPUFlags::ZERO, result == 0);
                flags.remove(CPUFlags::HALF_CARRY);

                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(result),
                    write_to: RegisterA,
                    flags,
                });
            }

            // JR Z, e8 | 2 12/8 | - - - -
            // Relative Jump to address n16 if condition cc is met.
            // Z - Execute if Z is set.
            0x28 => {
                let offset = self.read_next_pc() as i8;
                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        let destination = self.program_counter.wrapping_add(offset as i16 as u16);

                        self.operation_queue.push_back(ReadWrite {
                            read_from: Immediate(destination.low()),
                            write_to: ProgramCounterLow,
                        });
                        self.operation_queue.push_back(ReadWrite {
                            read_from: Immediate(destination.high()),
                            write_to: ProgramCounterHigh,
                        });
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // ADD HL, HL | 1 8 | - 0 H C
            // Add the value in HL to HL.
            0x29 => {
                let half_carry = check_half_carry_add_u16(self.get_hl(), self.get_hl());
                let (result, carry) = self.get_hl().overflowing_add(self.get_hl());

                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(result.low()),
                    write_to: RegisterL,
                });
                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(result.high()),
                    write_to: RegisterH,
                    flags,
                });
            }

            // LD A, [HL+] | 1 8 | - - - -
            // Copy the byte pointed to by HL into register A, and increment HL afterwards.
            0x2A => {
                let address = self.get_hl();
                let value = self.get_hl().wrapping_add(1);
                self.operation_queue.extend([
                    Fetch,
                    Parallel(3),
                    ReadWrite {
                        read_from: MemoryAddress(address),
                        write_to: RegisterA,
                    },
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // DEC HL | 1 8 | - - - -
            // Decrement HL by 1
            0x2B => {
                let value = self.get_hl().wrapping_sub(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // INC L | 1 4 | Z 0 H -
            // Increment the value in register L by 1.
            0x2C => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterL,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC L | 1 4 | Z 1 H -
            // Decrement the value in register L by 1.
            0x2D => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterL,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD L, n8 | 2 8 | - - - -
            // Copy the value n8 into register L.
            0x2E => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: Immediate(value),
                        write_to: RegisterL,
                    },
                ]);
            }

            // CPL | 1 4 | - 1 1 -
            // ComPLement accumulator (A = ~A); also called bitwise NOT
            0x2F => {
                let result = !self.register_a;

                let mut flags = self.register_f;
                flags.insert(CPUFlags::SUBTRACT | CPUFlags::HALF_CARRY);

                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(result),
                    write_to: RegisterA,
                    flags,
                });
            }

            // JR NC, e8 | 2 12/8 | - - - -
            // Relative Jump to address e8 if condition cc is met.
            // NC - Execute if C is not set.
            0x30 => {
                let offset = self.read_next_pc() as i8;

                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        let destination = self.program_counter.wrapping_add(offset as i16 as u16);
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(destination.low()),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(destination.high()),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                }
            }

            // LD SP, n16 | 3 12 | - - - -
            // Copy n16 into register SP.
            0x31 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: Immediate(low),
                        write_to: StackPointerLow,
                    },
                    ReadWrite {
                        read_from: Immediate(high),
                        write_to: StackPointerHigh,
                    },
                ]);
            }

            // LD [HL-], A | 1 8 | - - - -
            // Copy the value in register A into the byte pointed by HL and decrement HL afterwards.
            0x32 => {
                let address = self.get_hl();
                let value = self.get_hl().wrapping_sub(1);
                self.operation_queue.extend([
                    Fetch,
                    Parallel(3),
                    ReadWrite {
                        read_from: RegisterA,
                        write_to: MemoryAddress(address),
                    },
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // INC SP | 1 8 | - - - -
            // Increment the value in register SP by 1.
            0x33 => {
                let value = self.stack_pointer.wrapping_add(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: StackPointerLow,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: StackPointerHigh,
                    },
                ]);
            }

            // INC [HL] | 1 12 | Z 0 H -
            // Increment the byte pointed to by HL by 1.
            0x34 => {
                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: MemoryAddress(self.get_hl()),
                        write_to: InternalBuffer,
                    },
                    Parallel(2),
                    Arithmetic {
                        lhs: InternalBuffer,
                        operation: Increment,
                        rhs: None,
                    },
                    ReadWrite {
                        read_from: InternalBuffer,
                        write_to: MemoryAddress(self.get_hl()),
                    },
                ]);
            }

            // DEC [HL] | 1 12 | Z 1 H -
            // Decrement the byte pointed to by HL by 1.
            0x35 => {
                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: MemoryAddress(self.get_hl()),
                        write_to: InternalBuffer,
                    },
                    Parallel(2),
                    Arithmetic {
                        lhs: InternalBuffer,
                        operation: Decrement,
                        rhs: None,
                    },
                    ReadWrite {
                        read_from: InternalBuffer,
                        write_to: MemoryAddress(self.get_hl()),
                    },
                ]);
            }

            // LD [HL], n8 | 2 12 | - - - -
            // Copy the value n8 into the byte pointed to by HL.
            0x36 => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    ReadWrite {
                        read_from: Immediate(value),
                        write_to: MemoryAddress(self.get_hl()),
                    },
                ]);
            }

            // SCF | 1 4 | - 0 0 1
            // Set Carry Flag
            0x37 => {
                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.remove(CPUFlags::HALF_CARRY);
                flags.insert(CPUFlags::CARRY);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(flags.bits()),
                    write_to: RegisterF,
                });
            }

            // JR C, e8 | 2 12/8 | - - - -
            // Relative Jump to address n16 if condition cc is met.
            // C - Execute if C is set.
            0x38 => {
                let offset = self.read_next_pc() as i8;

                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        let destination = self.program_counter.wrapping_add(offset as i16 as u16);
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(destination.low()),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(destination.high()),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // ADD HL, SP | 1 8 | - 0 H C
            // Add the value in SP to HL.
            // H - Set if overflow from bit 11
            // C - Set if overflow from bit 15
            0x39 => {
                let half_carry = check_half_carry_add_u16(self.stack_pointer, self.get_hl());
                let (result, carry) = self.get_hl().overflowing_add(self.stack_pointer);

                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(result.low()),
                    write_to: RegisterL,
                });
                self.operation_queue.push_back(ReadWriteWithFlags {
                    read_from: Immediate(result.high()),
                    write_to: RegisterH,
                    flags,
                });
            }

            // LD A, [HL-] | 1 8 | - - - -
            // Copy the byte pointed to by HL into register A, and decrement HL afterwards.
            0x3A => {
                let address = self.get_hl();
                let value = self.get_hl().wrapping_sub(1);
                self.operation_queue.extend([
                    Fetch,
                    Parallel(3),
                    ReadWrite {
                        read_from: MemoryAddress(address),
                        write_to: RegisterA,
                    },
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: RegisterL,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: RegisterH,
                    },
                ]);
            }

            // DEC SP | 1 8 | - - - -
            // Decrement the value in register SP by 1
            0x3B => {
                let value = self.stack_pointer.wrapping_sub(1);
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: Immediate(value.low()),
                        write_to: StackPointerLow,
                    },
                    ReadWrite {
                        read_from: Immediate(value.high()),
                        write_to: StackPointerHigh,
                    },
                ]);
            }

            // INC A | 1 4 | Z 0 H -
            // Increment the value in register A by 1.
            0x3C => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation: Increment,
                    rhs: None,
                });
            }

            // DEC A | 1 4 | Z 1 H -
            // Decrement the value in register A by 1.
            0x3D => {
                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation: Decrement,
                    rhs: None,
                });
            }

            // LD A, n8 | 1 8 | - - - -
            // Copy the value n8 to register A
            0x3E => {
                let value = self.read_next_pc();

                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: Immediate(value),
                        write_to: RegisterA,
                    },
                ]);
            }

            // CCF | 1 4 | - 0 0 C
            // Complement Carry Flag
            0x3F => {
                let mut flags = self.register_f;
                flags.remove(CPUFlags::SUBTRACT);
                flags.remove(CPUFlags::HALF_CARRY);
                flags.toggle(CPUFlags::CARRY);

                self.operation_queue.push_back(ReadWrite {
                    read_from: Immediate(flags.bits()),
                    write_to: RegisterF,
                });
            }

            // HALT | 1 4 | - - - -
            // Enter CPU low-power consumption mode until an interrupt occurs.
            0x76 => {
                if self.ime != IMEState::Enabled
                    && self.read_memory(0xFFFF) & self.read_memory(0xFF0F) != 0
                {
                    self.state = CPUState::HaltBug;
                } else {
                    self.state = CPUState::Halted;
                }
                self.operation_queue.push_back(Nop);
            }

            // LD r8, r8
            0x40..=0x7F => {
                let read_from = match next_instruction & 0b0000_0111 {
                    0 => RegisterB,
                    1 => RegisterC,
                    2 => RegisterD,
                    3 => RegisterE,
                    4 => RegisterH,
                    5 => RegisterL,
                    6 => {
                        self.operation_queue.push_back(Fetch);
                        MemoryAddress(self.get_hl())
                    }
                    7 => RegisterA,
                    _ => None,
                };

                let write_to = match (next_instruction & 0b0011_1000) >> 3 {
                    0 => RegisterB,
                    1 => RegisterC,
                    2 => RegisterD,
                    3 => RegisterE,
                    4 => RegisterH,
                    5 => RegisterL,
                    6 => {
                        self.operation_queue.push_back(Fetch);
                        MemoryAddress(self.get_hl())
                    }
                    7 => RegisterA,
                    _ => None,
                };

                self.operation_queue.push_back(ReadWrite {
                    read_from,
                    write_to,
                });
            }

            // ADD, ADC, SUB, SBC, AND, XOR, OR, CP
            0x80..=0xBF => {
                let rhs = match next_instruction & 0b0000_0111 {
                    0 => RegisterB,
                    1 => RegisterC,
                    2 => RegisterD,
                    3 => RegisterE,
                    4 => RegisterH,
                    5 => RegisterL,
                    6 => {
                        self.operation_queue.push_back(Fetch);
                        MemoryAddress(self.get_hl())
                    }
                    7 => RegisterA,
                    _ => None,
                };

                let operation = match (next_instruction & 0b0011_1000) >> 3 {
                    0 => Add,
                    1 => AddWithCarry,
                    2 => Sub,
                    3 => SubWithCarry,
                    4 => And,
                    5 => Xor,
                    6 => Or,
                    _ => Compare,
                };

                self.operation_queue.push_back(Arithmetic {
                    lhs: RegisterA,
                    operation,
                    rhs,
                });
            }

            // RET NZ | 1 8/20 | - - - -
            // Return from subroutine if condition cc is met.
            // This pops the address from the stack pointer and sets the program counter.
            // NZ - Execute if Z is not set
            0xC0 => {
                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        self.operation_queue.extend([
                            Internal,
                            StackPop(ProgramCounterLow),
                            StackPop(ProgramCounterHigh),
                            Internal,
                        ]);
                    }
                }
            }

            // POP BC | 1 12 | - - - -
            // Pop register BC from the stack.
            0xC1 => {
                self.operation_queue
                    .extend([Fetch, StackPop(RegisterC), StackPop(RegisterB)]);
            }

            // JP NZ, a16 | 3 12/16 | - - - -
            // Jump to address n16 if condition cc is met.
            // NZ - Execute if Z is not set
            0xC2 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                }
            }

            // JP a16 | 3 16 | - - - -
            // Effectively copy n16 into PC
            0xC3 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Fetch,
                    ReadWrite {
                        read_from: Immediate(low),
                        write_to: ProgramCounterLow,
                    },
                    ReadWrite {
                        read_from: Immediate(high),
                        write_to: ProgramCounterHigh,
                    },
                ]);
            }

            // CALL NZ, a16 | 3 12/24 | - - - -
            // Call address u16 id condition cc is met.
            // This pushes the address of the instruction after the call on the stack, such
            // that RET can pop it later; then executes an implicit JP u16
            // NZ - Execute if Z not set
            0xC4 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        self.operation_queue.extend([
                            StackPush(ProgramCounterHigh),
                            StackPush(ProgramCounterLow),
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                }
            }

            // PUSH BC | 1 16 | - - - -
            // Push the contents of register BC to the stack.
            0xC5 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    StackPush(RegisterB),
                    StackPush(RegisterC),
                ]);
            }

            // ADD A, n8 | 2 8 | Z 0 H C
            // Add n8 to register A
            0xC6 => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        rhs: Immediate(value),
                        operation: Add,
                    },
                ]);
            }

            // RST $00 | 1 16 | - - - -
            // Essentially acts as a CALL 0x0000
            0xC7 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // RET Z | 1 20/8 | - - - -
            // Return from subroutine if condition cc is met.
            // Z - Zero flag is set
            0xC8 => {
                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.extend([
                            Internal,
                            StackPop(ProgramCounterLow),
                            StackPop(ProgramCounterHigh),
                            Internal,
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // RET | 1 16 | - - - -
            // Return from subroutine. This is basically a POP PC (if such an instruction existed).
            0xC9 => {
                self.operation_queue.extend([
                    Fetch,
                    StackPop(ProgramCounterLow),
                    StackPop(ProgramCounterHigh),
                    Internal,
                ]);
            }

            // JP Z, a16 | 3 16/12 | - - - -
            0xCA => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // CALL Z, a16 | 3  24/12 | - - - -
            // Call address n16 if condition cc is met.
            // Z - True is zero flag is set
            0xCC => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::ZERO) {
                    true => {
                        self.operation_queue.extend([
                            StackPush(ProgramCounterHigh),
                            StackPush(ProgramCounterLow),
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // CALL a16 | 3 24 | - - - -
            // Call address a16
            0xCD => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([
                    Fetch,
                    Fetch,
                    StackPush(ProgramCounterHigh),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(low),
                        write_to: ProgramCounterLow,
                    },
                    ReadWrite {
                        read_from: Immediate(high),
                        write_to: ProgramCounterHigh,
                    },
                ]);
            }

            // ADC A, n8 | 2 8 | Z 0 H C
            // Add n8 to register A with carry
            0xCE => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        rhs: Immediate(value),
                        operation: AddWithCarry,
                    },
                ]);
            }

            // RST $08 | 1 16 | - - - -
            // Essentially acts as a CALL 0x0008
            0xCF => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x08),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // RET NC | 1 8/20 | - - - -
            // Return from call if carry flag not set
            0xD0 => {
                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => self.operation_queue.extend([
                        Internal,
                        StackPop(ProgramCounterLow),
                        StackPop(ProgramCounterHigh),
                        Internal,
                    ]),
                }
            }

            // POP DE | 1 12 | - - - -
            // Pop the value at the address in SP to register DE, increment SP by 2
            0xD1 => {
                self.operation_queue
                    .extend([Fetch, StackPop(RegisterE), StackPop(RegisterD)]);
            }

            // JP NC, a16 | 3 12/16 | - - - -
            // Copy a16 to PC if carry not set
            0xD2 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                }
            }

            // CALL NC, a16 | 3 12/24 | - - - -
            // Push PC to [SP], decrement SP, and set PC to a16 if carry not set
            0xD4 => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.push_back(Internal);
                    }
                    false => {
                        self.operation_queue.extend([
                            StackPush(ProgramCounterHigh),
                            StackPush(ProgramCounterLow),
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                }
            }

            // PUSH DE | 1 16 | - - - -
            // Push the contents of register DE to the stack and decrement SP
            0xD5 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    StackPush(RegisterD),
                    StackPush(RegisterE),
                ]);
            }

            // SUB A, u8 | 2 8 | Z 1 H C
            // Subtract u8 from register A
            0xD6 => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: Sub,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $10 | 1 16 | - - - -
            // Call address $10. Shorter and faster than using CALL for certain addresses.
            0xD7 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x10),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // RET C | 1 8/20 | - - - -
            // Return from subroutine if carry set
            0xD8 => {
                self.operation_queue.push_back(Fetch);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => self.operation_queue.extend([
                        Internal,
                        StackPop(ProgramCounterLow),
                        StackPop(ProgramCounterHigh),
                        Internal,
                    ]),
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // RETI | 1 16 | - - - -
            // Return from subroutine and enable interrupts. Equivalent to executing EI then
            // RET, meaning that IME is set right after this instruction.
            0xD9 => {
                trace!("RETI - Returning from interrupt.");
                self.ime = IMEState::Enabled;
                self.operation_queue.extend([
                    Fetch,
                    StackPop(ProgramCounterLow),
                    StackPop(ProgramCounterHigh),
                    Internal,
                ]);
            }

            // JP C, a16 | 3 12/16 | - - - -
            // Set PC to address a16 if carry set
            0xDA => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.extend([
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // CALL C, a16 | 3 12/24 | - - - -
            // If carry set, push PC to [SP], decrement SP, and set PC to a16
            0xDC => {
                let low = self.read_next_pc();
                let high = self.read_next_pc();

                self.operation_queue.extend([Fetch, Fetch]);

                match self.register_f.contains(CPUFlags::CARRY) {
                    true => {
                        self.operation_queue.extend([
                            StackPush(ProgramCounterHigh),
                            StackPush(ProgramCounterLow),
                            ReadWrite {
                                read_from: Immediate(low),
                                write_to: ProgramCounterLow,
                            },
                            ReadWrite {
                                read_from: Immediate(high),
                                write_to: ProgramCounterHigh,
                            },
                        ]);
                    }
                    false => {
                        self.operation_queue.push_back(Internal);
                    }
                }
            }

            // SBC A, u8 | 2 8 | Z 1 H C
            // Subtract u8 from A with carry
            0xDE => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: SubWithCarry,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $18 | 1 16 | - - - -
            // Call address $18. Shorter and faster than using CALL for certain addresses.
            0xDF => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x18),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // LD [FF00 + a8], A | 2 12 | - - - -
            // Load the value in register A to the memory address at $FF00 + a8
            0xE0 => {
                let offset = self.read_next_pc() as u16;
                let address = 0xFF00 + offset;

                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    ReadWrite {
                        read_from: RegisterA,
                        write_to: MemoryAddress(address),
                    },
                ]);
            }

            // POP HL | 1 12 | - - - -
            // Pop register HL from the stack.
            0xE1 => {
                self.operation_queue
                    .extend([Fetch, StackPop(RegisterL), StackPop(RegisterH)]);
            }

            // LD [FF00 + C], A | 1 8 | - - - -
            // Load the value in register A to the memory address at $FF00 + the value at
            // register C
            0xE2 => {
                let address = 0xFF00 + self.register_c as u16;

                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: RegisterA,
                        write_to: MemoryAddress(address),
                    },
                ]);
            }

            // PUSH HL | 1 16 | - - - -
            // Push the value in register HL to the stack.
            0xE5 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    StackPush(RegisterH),
                    StackPush(RegisterL),
                ]);
            }

            // AND A, u8 | 2 8 | Z 0 1 0
            // Set A to the bitwise AND between the value u8 and A.
            0xE6 => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: And,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $20 | 1 16 | - - - -
            // Call address $20. Shorter and faster than using CALL for certain addresses.
            0xE7 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x20),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // ADD SP, i8 | 2 16 | 0 0 H C
            // Add the signed value i8 to SP
            0xE8 => {
                let offset_byte = self.read_next_pc();
                let offset = offset_byte as i8;
                let old_sp = self.stack_pointer;

                let new_sp = old_sp.wrapping_add(offset as u16);

                let half_carry = ((old_sp & 0xF) + ((offset as u16) & 0xF)) > 0xF;
                let carry = ((old_sp & 0xFF) + ((offset as u16) & 0xFF)) > 0xFF;

                let mut flags = self.register_f;
                flags.remove(CPUFlags::ZERO | CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    ReadWrite {
                        read_from: Immediate(new_sp.low()),
                        write_to: StackPointerLow,
                    },
                    ReadWriteWithFlags {
                        read_from: Immediate(new_sp.high()),
                        write_to: StackPointerHigh,
                        flags,
                    },
                ]);
            }

            // JP HL | 1 4 | - - - -
            // Jump to address in HL - effectively copy the value in register HL to PC
            0xE9 => {
                self.operation_queue.extend([
                    Parallel(2),
                    ReadWrite {
                        read_from: RegisterL,
                        write_to: ProgramCounterLow,
                    },
                    ReadWrite {
                        read_from: RegisterH,
                        write_to: ProgramCounterHigh,
                    },
                ]);
            }

            // LD [n16], A | 3 16 | - - - -
            // Copy the value in register A to the byte at address n16
            0xEA => {
                let address = self.read_next_pc_u16();

                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Internal,
                    ReadWrite {
                        read_from: RegisterA,
                        write_to: MemoryAddress(address),
                    },
                ]);
            }

            // XOR A, n8 | 2 8 | Z 0 0 0
            // Set A to the bitwise XOR between the value n8 and A.
            0xEE => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: Xor,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $28 | 1 16 | - - - -
            // Call address $28. Shorter and faster than using CALL for certain addresses.
            0xEF => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x28),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // LD A, [FF00 + u8] | 2 12 | - - - -
            // Copy the value at $FF00 + u8 to register A
            0xF0 => {
                let value = self.read_next_pc() as u16;
                let address = 0xFF00 + value;

                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    ReadWrite {
                        read_from: MemoryAddress(address),
                        write_to: RegisterA,
                    },
                ]);
            }

            // POP AF | 1 12 | Z N H C
            // Pop register AF from the stack.
            0xF1 => {
                self.operation_queue
                    .extend([Fetch, StackPop(RegisterF), StackPop(RegisterA)]);
            }

            // LD A, [FF00 + C] | 1 8 | - - - -
            0xF2 => {
                let address = 0xFF00 + (self.register_c as u16);
                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: MemoryAddress(address),
                        write_to: RegisterA,
                    },
                ]);
            }

            // DI | 1 4 | - - - -
            // Disable interrupts by clearing the IME flag.
            0xF3 => {
                trace!("Interrupts disabled.");
                self.ime = IMEState::Disabled;
                self.operation_queue.push_back(Nop);
            }

            // PUSH AF | 1 16 | - - - -
            // Push the contents of the AF register to the stack.
            0xF5 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    StackPush(RegisterA),
                    StackPush(RegisterF),
                ]);
            }

            // OR A, u8 | 2 8 | Z 0 0 0
            0xF6 => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: Or,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $30 | 1 16 | - - - -
            // Call address $30. Shorter and faster than using CALL for certain addresses.
            0xF7 => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x30),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // LD HL, SP+i8 | 2 12 | 0 0 H C
            // Add the signed value i8 to SP and copy the result in HL
            0xF8 => {
                let offset_byte = self.read_next_pc();
                let offset = offset_byte as i8;
                let old_sp = self.stack_pointer;
                let new_val = old_sp.wrapping_add(offset as u16);
                let half_carry = ((old_sp & 0xF) + ((offset as u16) & 0xF)) > 0xF;
                let carry = ((old_sp & 0xFF) + ((offset as u16) & 0xFF)) > 0xFF;

                // self.set_hl(new_val);
                let mut flags = self.register_f;
                flags.remove(CPUFlags::ZERO | CPUFlags::SUBTRACT);
                flags.set(CPUFlags::HALF_CARRY, half_carry);
                flags.set(CPUFlags::CARRY, carry);

                self.operation_queue.extend([
                    Fetch,
                    ReadWrite {
                        read_from: Immediate(new_val.low()),
                        write_to: RegisterL,
                    },
                    ReadWriteWithFlags {
                        read_from: Immediate(new_val.high()),
                        write_to: RegisterH,
                        flags,
                    },
                ]);
            }

            // LD SP, HL | 1 8 | - - - -
            // Copy register HL into SP
            0xF9 => {
                self.operation_queue.extend([
                    ReadWrite {
                        read_from: RegisterL,
                        write_to: StackPointerLow,
                    },
                    ReadWrite {
                        read_from: RegisterH,
                        write_to: StackPointerHigh,
                    },
                ]);
            }

            // LD A, [n16] | 3 16 | - - - -
            // Copy the byte at address n16 into register A
            0xFA => {
                let address = self.read_next_pc_u16();
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Internal,
                    ReadWrite {
                        read_from: MemoryAddress(address),
                        write_to: RegisterA,
                    },
                ]);
            }

            // EI | 1 4 | - - - -
            // Enable interrupts by setting the IME flag. The flag is only set _after_ the
            // instruction following EI.
            0xFB => {
                trace!("Interrupts enabled.");
                if self.ime == IMEState::Disabled {
                    self.ime = IMEState::WillEnable;
                }
                self.operation_queue.push_back(Nop);
            }

            // CP A, n8 | 2 8 | Z 1 H C
            // Compare the value in A with the value n8. This subtracts the value n8 from A and
            // sets flags accordingly, but discards the result.
            0xFE => {
                let value = self.read_next_pc();
                self.operation_queue.extend([
                    Fetch,
                    Arithmetic {
                        lhs: RegisterA,
                        operation: Compare,
                        rhs: Immediate(value),
                    },
                ]);
            }

            // RST $38 | 1 16 | - - - -
            // Call address $38. Shorter and faster than using CALL for certain addresses.
            0xFF => {
                self.operation_queue.extend([
                    Fetch,
                    Internal,
                    Parallel(2),
                    StackPush(ProgramCounterHigh),
                    ReadWrite {
                        read_from: Immediate(0x00),
                        write_to: ProgramCounterHigh,
                    },
                    Parallel(2),
                    StackPush(ProgramCounterLow),
                    ReadWrite {
                        read_from: Immediate(0x38),
                        write_to: ProgramCounterLow,
                    },
                ]);
            }

            // PREFIX
            0xCB => {
                let prefix_instruction = self.read_next_pc();

                self.operation_queue.push_back(Fetch);

                match prefix_instruction {
                    // RLC, RRC, RL, RR, SLA, SRA, SWAP, SRL
                    0x00..=0x3F => {
                        let lhs = match prefix_instruction & 0b0000_0111 {
                            0 => RegisterB,
                            1 => RegisterC,
                            2 => RegisterD,
                            3 => RegisterE,
                            4 => RegisterH,
                            5 => RegisterL,
                            6 => {
                                self.operation_queue.extend([
                                    Fetch,
                                    ReadWrite {
                                        read_from: MemoryAddress(self.get_hl()),
                                        write_to: InternalBuffer,
                                    },
                                ]);
                                InternalBuffer
                            }
                            _ => RegisterA,
                        };

                        let operation = match (prefix_instruction & 0b0011_1000) >> 3 {
                            0 => RotateLeft {
                                set_zero: true,
                                through_carry: false,
                            },
                            1 => RotateRight {
                                set_zero: true,
                                through_carry: false,
                            },
                            2 => RotateLeft {
                                set_zero: true,
                                through_carry: true,
                            },
                            3 => RotateRight {
                                set_zero: true,
                                through_carry: true,
                            },
                            4 => ShiftLeft,
                            5 => ShiftRight {
                                arithmetically: true,
                            },
                            6 => Swap,
                            _ => ShiftRight {
                                arithmetically: false,
                            },
                        };

                        match lhs == InternalBuffer {
                            true => {
                                self.operation_queue.extend([
                                    Parallel(2),
                                    Arithmetic {
                                        lhs,
                                        operation,
                                        rhs: None,
                                    },
                                    ReadWrite {
                                        read_from: InternalBuffer,
                                        write_to: MemoryAddress(self.get_hl()),
                                    },
                                ]);
                            }
                            false => {
                                self.operation_queue.push_back(Arithmetic {
                                    lhs,
                                    operation,
                                    rhs: None,
                                });
                            }
                        }
                    }

                    // BIT x, r8 | 2 8 | Z 0 1 -
                    // Test bit u3 in register r8, set the zero flag if bit not set
                    0x40..=0x7F => {
                        let lhs = match prefix_instruction & 0b0000_0111 {
                            0 => RegisterB,
                            1 => RegisterC,
                            2 => RegisterD,
                            3 => RegisterE,
                            4 => RegisterH,
                            5 => RegisterL,
                            6 => {
                                self.operation_queue.push_back(Fetch);
                                MemoryAddress(self.get_hl())
                            }
                            _ => RegisterA,
                        };

                        let bit = (prefix_instruction & 0b0011_1000) >> 3;

                        self.operation_queue.push_back(Arithmetic {
                            lhs,
                            operation: Bit(bit),
                            rhs: None,
                        });
                    }

                    // RES x, r8 | 2 8 | - - - -
                    // Set bit u3 in register r8 to 0. Bit 0 is the rightmost one, bit 7 is the
                    // leftmost one
                    0x80..=0xBF => {
                        let lhs = match prefix_instruction & 0b0000_0111 {
                            0 => RegisterB,
                            1 => RegisterC,
                            2 => RegisterD,
                            3 => RegisterE,
                            4 => RegisterH,
                            5 => RegisterL,
                            6 => {
                                self.operation_queue.extend([
                                    Fetch,
                                    ReadWrite {
                                        read_from: MemoryAddress(self.get_hl()),
                                        write_to: InternalBuffer,
                                    },
                                ]);
                                InternalBuffer
                            }
                            _ => RegisterA,
                        };

                        let bit = (prefix_instruction & 0b0011_1000) >> 3;

                        match lhs == InternalBuffer {
                            true => {
                                self.operation_queue.extend([
                                    Parallel(2),
                                    Arithmetic {
                                        lhs,
                                        operation: Res(bit),
                                        rhs: None,
                                    },
                                    ReadWrite {
                                        read_from: InternalBuffer,
                                        write_to: MemoryAddress(self.get_hl()),
                                    },
                                ]);
                            }
                            false => {
                                self.operation_queue.push_back(Arithmetic {
                                    lhs,
                                    operation: Res(bit),
                                    rhs: None,
                                });
                            }
                        }
                    }

                    // SET x, r8 | 2 8 | - - - -
                    // Set bit u3 in register r8 to 1. Bit 0 is the rightmost one, bit 7 is the
                    // leftmost one
                    0xC0..=0xFF => {
                        let lhs = match prefix_instruction & 0b0000_0111 {
                            0 => RegisterB,
                            1 => RegisterC,
                            2 => RegisterD,
                            3 => RegisterE,
                            4 => RegisterH,
                            5 => RegisterL,
                            6 => {
                                self.operation_queue.extend([
                                    Fetch,
                                    ReadWrite {
                                        read_from: MemoryAddress(self.get_hl()),
                                        write_to: InternalBuffer,
                                    },
                                ]);
                                InternalBuffer
                            }
                            _ => RegisterA,
                        };

                        let bit = (prefix_instruction & 0b0011_1000) >> 3;

                        match lhs == InternalBuffer {
                            true => {
                                self.operation_queue.extend([
                                    Parallel(2),
                                    Arithmetic {
                                        lhs,
                                        operation: Set(bit),
                                        rhs: None,
                                    },
                                    ReadWrite {
                                        read_from: InternalBuffer,
                                        write_to: MemoryAddress(self.get_hl()),
                                    },
                                ]);
                            }
                            false => {
                                self.operation_queue.push_back(Arithmetic {
                                    lhs,
                                    operation: Set(bit),
                                    rhs: None,
                                });
                            }
                        }
                    }
                }
            }

            _ => panic!("invalid instruction"),
        }
    }

    fn get_bc(&self) -> u16 {
        ((self.register_b as u16) << 8) | (self.register_c as u16)
    }

    fn get_de(&self) -> u16 {
        ((self.register_d as u16) << 8) | (self.register_e as u16)
    }

    fn get_hl(&self) -> u16 {
        ((self.register_h as u16) << 8) | (self.register_l as u16)
    }

    fn read_next_pc(&mut self) -> u8 {
        let byte = self.memory.read().unwrap().read_byte(self.program_counter);
        self.program_counter = self.program_counter.wrapping_add(1);
        byte
    }

    fn read_next_pc_u16(&mut self) -> u16 {
        let low = self.read_next_pc() as u16;
        let high = self.read_next_pc() as u16;
        (high << 8) | low
    }

    fn read_memory(&self, address: u16) -> u8 {
        self.memory.read().unwrap().read_byte(address)
    }

    fn write_memory(&self, address: u16, value: u8) {
        self.memory.write().unwrap().write_byte(address, value);
    }
}

fn check_half_carry_add(a: u8, b: u8) -> bool {
    ((a & 0xF) + (b & 0xF)) > 0xF
}

fn check_half_carry_sub(a: u8, b: u8) -> bool {
    (a & 0xF) < (b & 0xF)
}

fn check_half_carry_add_u16(a: u16, b: u16) -> bool {
    ((a & 0xFFF) + (b & 0xFFF)) > 0xFFF
}

fn check_half_carry_adc(a: u8, b: u8, c: bool) -> bool {
    ((a & 0xF) + (b & 0xF) + (c as u8)) > 0xF
}

fn check_half_carry_sbc(a: u8, b: u8, c: bool) -> bool {
    (a & 0xF) < ((b & 0xF) + (c as u8))
}

#[derive(Debug, PartialEq, Clone)]
enum IMEState {
    Disabled,
    WillEnable,
    Enabled,
}

#[derive(Debug, PartialEq, Clone)]
enum CPUState {
    Ready,
    HaltBug,
    Halted,
}

#[derive(Debug, PartialEq, Clone)]
enum InterruptDispatchState {
    Waiting,
    Dispatching {
        operations_remaining: u8,
    },
    Cancelling,
    Finalizing {
        interrupt_enable: u8,
        interrupt_flag: u8,
    },
}

bitflags! {
    #[repr(transparent)]
    #[derive(Debug, PartialEq, Clone, Copy)]
    struct CPUFlags: u8 {
        const ZERO = 0b1000_0000;
        const SUBTRACT = 0b0100_0000;
        const HALF_CARRY = 0b0010_0000;
        const CARRY = 0b0001_0000;
    }
}

#[derive(Debug, PartialEq, Clone, Copy)]
enum Operation {
    ReadWriteWithFlags {
        read_from: OpTarget,
        write_to: OpTarget,
        flags: CPUFlags,
    },
    Arithmetic {
        lhs: OpTarget,
        operation: ArithmeticOperation,
        rhs: OpTarget,
    },
    ReadWrite {
        read_from: OpTarget,
        write_to: OpTarget,
    },
    Parallel(u8),
    StackPush(OpTarget),
    StackPop(OpTarget),
    Nop,
    Fetch,
    Internal,
    InterruptDispatch,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum OpTarget {
    MemoryAddress(u16),
    Immediate(u8),
    RegisterA,
    RegisterB,
    RegisterC,
    RegisterD,
    RegisterE,
    RegisterF,
    RegisterH,
    RegisterL,
    ProgramCounterLow,
    ProgramCounterHigh,
    StackPointerLow,
    StackPointerHigh,
    InternalBuffer,
    None,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum ArithmeticOperation {
    Increment,
    Decrement,
    RotateLeft { set_zero: bool, through_carry: bool },
    RotateRight { set_zero: bool, through_carry: bool },
    Add,
    Sub,
    AddWithCarry,
    SubWithCarry,
    And,
    Xor,
    Or,
    Compare,
    ShiftLeft,
    ShiftRight { arithmetically: bool },
    Swap,
    Bit(u8),
    Res(u8),
    Set(u8),
}

pub trait SplitBytes {
    fn low(&self) -> u8;
    fn high(&self) -> u8;
}

impl SplitBytes for u16 {
    fn low(&self) -> u8 {
        (self & 0xFF) as u8
    }

    fn high(&self) -> u8 {
        ((self & 0xFF00) >> 8) as u8
    }
}

pub struct OperationQueue<T, const CAP: usize>(ArrayDeque<T, CAP, Saturating>);

impl<T, const CAP: usize> Deref for OperationQueue<T, CAP> {
    type Target = ArrayDeque<T, CAP, Saturating>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T, const CAP: usize> DerefMut for OperationQueue<T, CAP> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T, const CAP: usize> OperationQueue<T, CAP> {
    pub fn new() -> Self {
        OperationQueue(ArrayDeque::new())
    }
    pub fn push_back(&mut self, element: T) {
        self.0.push_back(element).unwrap()
    }

    pub fn pop_front(&mut self) -> Option<T> {
        self.0.pop_front()
    }

    pub fn extend<I>(&mut self, iter: I)
    where
        I: IntoIterator<Item = T>,
    {
        self.0.extend_back(iter);
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }
}

impl<T, const CAP: usize> Default for OperationQueue<T, CAP> {
    fn default() -> Self {
        Self::new()
    }
}
