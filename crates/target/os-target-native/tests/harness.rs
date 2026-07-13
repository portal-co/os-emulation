//! Shared test harness for running generated machine code under Unicorn.

use std::fmt;

use unicorn_engine::unicorn_const::{Arch, Mode, Prot};
use unicorn_engine::RegisterX86;
use unicorn_engine::Unicorn;

/// Run x86-64 machine code and return the value left in RAX.
///
/// The code is mapped at `0x100000`, the stack at `0x200000`.
/// A synthetic return address (`code_base + code.len()`) is pushed onto the
/// stack so that a generated `ret` returns control to the harness.
pub fn run_x86_64(code: &[u8]) -> Result<u64, String> {
    const CODE_BASE: u64 = 0x100000;
    const STACK_BASE: u64 = 0x200000;
    const STACK_SIZE: u64 = 0x10000;

    let mut uc = Unicorn::new(Arch::X86, Mode::MODE_64).map_err(fmt_err)?;
    uc.mem_map(CODE_BASE, 0x10000, Prot::ALL).map_err(fmt_err)?;
    uc.mem_map(STACK_BASE, STACK_SIZE, Prot::ALL).map_err(fmt_err)?;
    uc.mem_write(CODE_BASE, code).map_err(fmt_err)?;

    let ret_addr = CODE_BASE + code.len() as u64;
    let rsp = STACK_BASE + STACK_SIZE - 8;
    uc.mem_write(rsp, &ret_addr.to_le_bytes()).map_err(fmt_err)?;
    uc.reg_write(RegisterX86::RSP, rsp).map_err(fmt_err)?;

    uc.emu_start(CODE_BASE, ret_addr, 0, 5000).map_err(fmt_err)?;
    uc.reg_read(RegisterX86::RAX).map_err(fmt_err)
}

fn fmt_err<E: fmt::Display>(e: E) -> String {
    format!("{e}")
}