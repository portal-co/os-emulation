//! Shared test harness for running generated machine code under Unicorn.
#![allow(dead_code)]

use std::fmt;

use unicorn_engine::unicorn_const::{Arch, Mode, Prot};
use unicorn_engine::{RegisterARM64, RegisterRISCV, RegisterX86, Unicorn};

pub const CODE_BASE: u64 = 0x100000;
pub const STACK_BASE: u64 = 0x200000;
pub const HEAP_BASE: u64 = 0x300000;
pub const STACK_SIZE: u64 = 0x10000;
pub const HEAP_SIZE: u64 = 0x10000;

fn write_u64(uc: &mut Unicorn<'static, ()>, addr: u64, value: u64) -> Result<(), String> {
    uc.mem_write(addr, &value.to_le_bytes()).map_err(fmt_err)
}

fn read_u64(uc: &mut Unicorn<'static, ()>, addr: u64) -> Result<u64, String> {
    let mut bytes = [0u8; 8];
    uc.mem_read(addr, &mut bytes).map_err(fmt_err)?;
    Ok(u64::from_le_bytes(bytes))
}

fn init_uc(arch: Arch, mode: Mode) -> Result<Unicorn<'static, ()>, String> {
    let mut uc = Unicorn::new(arch, mode).map_err(fmt_err)?;
    uc.mem_map(CODE_BASE, 0x10000, Prot::ALL).map_err(fmt_err)?;
    uc.mem_map(STACK_BASE, STACK_SIZE, Prot::ALL).map_err(fmt_err)?;
    uc.mem_map(HEAP_BASE, HEAP_SIZE, Prot::ALL).map_err(fmt_err)?;
    Ok(uc)
}

/// Run x86-64 machine code and return `(RAX, word_at_heap_base)`.
pub fn run_x86_64(code: &[u8], heap_word: u64) -> Result<(u64, u64), String> {
    let mut uc = init_uc(Arch::X86, Mode::MODE_64)?;
    uc.mem_write(CODE_BASE, code).map_err(fmt_err)?;
    write_u64(&mut uc, HEAP_BASE, heap_word)?;

    let ret_addr = CODE_BASE + code.len() as u64;
    let rsp = STACK_BASE + STACK_SIZE - 8;
    uc.mem_write(rsp, &ret_addr.to_le_bytes()).map_err(fmt_err)?;
    uc.reg_write(RegisterX86::RSP, rsp).map_err(fmt_err)?;

    uc.emu_start(CODE_BASE, ret_addr, 0, 5000).map_err(fmt_err)?;
    let rax = uc.reg_read(RegisterX86::RAX).map_err(fmt_err)?;
    let heap = read_u64(&mut uc, HEAP_BASE)?;
    Ok((rax, heap))
}

/// Run AArch64 machine code and return `(X0, word_at_heap_base)`.
pub fn run_aarch64(code: &[u8], heap_word: u64) -> Result<(u64, u64), String> {
    let mut uc = init_uc(Arch::ARM64, Mode::LITTLE_ENDIAN)?;
    uc.mem_write(CODE_BASE, code).map_err(fmt_err)?;
    write_u64(&mut uc, HEAP_BASE, heap_word)?;

    let ret_addr = CODE_BASE + code.len() as u64;
    let sp = STACK_BASE + STACK_SIZE - 16;
    uc.reg_write(RegisterARM64::SP, sp).map_err(fmt_err)?;
    uc.reg_write(RegisterARM64::LR, ret_addr).map_err(fmt_err)?;

    uc.emu_start(CODE_BASE, ret_addr, 0, 5000).map_err(fmt_err)?;
    let x0 = uc.reg_read(RegisterARM64::X0).map_err(fmt_err)?;
    let heap = read_u64(&mut uc, HEAP_BASE)?;
    Ok((x0, heap))
}

/// Run RISC-V 64 machine code and return `(A0, word_at_heap_base)`.
pub fn run_riscv64(code: &[u8], heap_word: u64) -> Result<(u64, u64), String> {
    let mut uc = init_uc(Arch::RISCV, Mode::RISCV64)?;
    uc.mem_write(CODE_BASE, code).map_err(fmt_err)?;
    write_u64(&mut uc, HEAP_BASE, heap_word)?;

    let ret_addr = CODE_BASE + code.len() as u64;
    let sp = STACK_BASE + STACK_SIZE - 16;
    uc.reg_write(RegisterRISCV::SP, sp).map_err(fmt_err)?;
    uc.reg_write(RegisterRISCV::RA, ret_addr).map_err(fmt_err)?;

    uc.emu_start(CODE_BASE, ret_addr, 0, 5000).map_err(fmt_err)?;
    let a0 = uc.reg_read(RegisterRISCV::A0).map_err(fmt_err)?;
    let heap = read_u64(&mut uc, HEAP_BASE)?;
    Ok((a0, heap))
}

fn fmt_err<E: fmt::Display>(e: E) -> String {
    format!("{e}")
}