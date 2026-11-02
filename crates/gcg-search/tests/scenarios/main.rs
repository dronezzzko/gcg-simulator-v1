//! The weakness scenario tests of Phase 6 (docs/port/PLAN.md §9.8, §12.2): one module per
//! scenario-shaped WEAKNESSES item, `<package>_<w-id>_<name>.rs`, built with the Rust testkit.

#[path = "../aiport/mod.rs"]
mod aiport;
#[path = "../common/mod.rs"]
mod common;

mod p6c_common;
mod p6c_w05_fusion;
mod p6c_w07_open_resources;
mod p6c_w139_held_trick;

mod p6e_common;
mod p6e_w09_ex_payment;
mod p6e_w12_hidden_burst;
mod p6e_w13_freedom;
mod p6e_w14_target_critic;

mod p6f_support;
mod p6f_w24_modal;
mod p6f_w28_end_phase;
mod p6f_w29_order;

mod p6a_w01_two_turn_plan;

mod p6b_support;
mod p6b_w03_last_blocker;
mod p6b_w16_fast_lethal;
mod p6b_w32_cant_block;
