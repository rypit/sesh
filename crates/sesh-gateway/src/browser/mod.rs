//! The browser (specs/010, ADR 0013): the gateway drives one Chromium per
//! agent over CDP, and is its only way online.

pub mod cdp;
pub mod egress;
pub mod sites;
pub mod totp;
