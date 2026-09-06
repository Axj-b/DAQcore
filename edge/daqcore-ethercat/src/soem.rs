// SPDX-License-Identifier: Apache-2.0
// Copyright 2026 DAQcore contributors

//! SOEM (Simple Open EtherCAT Master) backend.
//!
//! **Linux only, requires libsoem.** This is the integration point for real
//! hardware; it is NOT validated and the PDO-to-channel mapping is not yet
//! implemented (see `exchange`).
//!
//! Build with `--features soem` after installing libsoem.

#![allow(dead_code)]

use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::{c_int, c_void};

use daqcore_core::{Error, Result};

use crate::config::Channel;
use crate::master::EthercatMaster;
use crate::state::BusState;

// SOEM's simple global API (from ethercatmain.h / ethercattype.h).
extern "C" {
    fn ec_init(ifname: *const std::os::raw::c_char) -> c_int;
    fn ec_config_init(usetable: c_int) -> c_int;
    fn ec_config_map(io_map: *mut c_void) -> c_int;
    fn ec_statecheck(slave: c_int, reqstate: c_int, timeout: c_int) -> c_int;
    fn ec_send_processdata() -> c_int;
    fn ec_receive_processdata(timeout: c_int) -> c_int;
    fn ec_close();
}

// EtherCAT AL state code for Operational.
const EC_STATE_OPERATIONAL: c_int = 0x08;

pub struct SoemMaster {
    ifname: String,
    channels: Vec<Channel>,
    state: BusState,
}

impl SoemMaster {
    pub fn new(ifname: impl Into<String>, channels: Vec<Channel>) -> Self {
        Self {
            ifname: ifname.into(),
            channels,
            state: BusState::Init,
        }
    }
}

impl EthercatMaster for SoemMaster {
    fn start(&mut self) -> Result<()> {
        let ifname = CString::new(self.ifname.as_str())
            .map_err(|_| Error::Config("invalid interface name".into()))?;
        unsafe {
            // 1. Open the raw socket on the NIC.
            if ec_init(ifname.as_ptr()) <= 0 {
                return Err(Error::Driver("ec_init failed".into()));
            }
            // 2. Scan slaves, read EEPROM (SII), build the PDO map.
            if ec_config_init(0) <= 0 {
                return Err(Error::Driver("ec_config_init failed".into()));
            }
            if ec_config_map(std::ptr::null_mut()) <= 0 {
                return Err(Error::Driver("ec_config_map failed".into()));
            }
            // 3. Drive all slaves through the state machine to OP.
            if ec_statecheck(0, EC_STATE_OPERATIONAL, 5_000_000) <= 0 {
                return Err(Error::Driver("bus did not reach OP".into()));
            }
        }
        self.state = BusState::Op;
        Ok(())
    }

    fn exchange(
        &mut self,
        outputs: &HashMap<String, f64>,
        inputs: &mut HashMap<String, f64>,
    ) -> Result<()> {
        // TODO(real hardware): write each output channel into its IOMap offset,
        // cycle, then read each input channel from its IOMap offset. The offset
        // table comes from `ec_slave[].outputs` / `inputs` populated by
        // `ec_config_map`.
        let _ = (outputs, inputs);
        unsafe {
            ec_send_processdata();
            ec_receive_processdata(2_000);
        }
        Ok(())
    }

    fn state(&self) -> BusState {
        self.state
    }

    fn stop(&mut self) -> Result<()> {
        unsafe {
            ec_close();
        }
        self.state = BusState::Init;
        Ok(())
    }

    fn channels(&self) -> &[Channel] {
        &self.channels
    }
}
