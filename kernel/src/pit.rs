use core::num::TryFromIntError;
use spin::Mutex;
use x86_64::instructions::port::{Port, PortWriteOnly};

const TIMER_FREQUENCY: u32 = 1_193_182;

pub static PIT0: ProgrammableIntervalTimer = ProgrammableIntervalTimer::new(Channel::Channel0);
pub static PIT1: ProgrammableIntervalTimer = ProgrammableIntervalTimer::new(Channel::Channel1);
pub static PIT2: ProgrammableIntervalTimer = ProgrammableIntervalTimer::new(Channel::Channel2);

pub struct ProgrammableIntervalTimer(Mutex<PIT>);

struct PIT {
    ch: Port<u8>,
    cmd: PortWriteOnly<u8>,
}

impl ProgrammableIntervalTimer {
    const fn new(ch: Channel) -> Self {
        Self(Mutex::new(PIT {
            ch: Port::new(ch.port()),
            cmd: PortWriteOnly::new(0x43),
        }))
    }

    fn set_cmd(cmd: &mut PortWriteOnly<u8>, channel: Channel, access_mode: AccessMode, operating_mode: OperatingMode) {
        let mut val = channel as u8;
        val |= (access_mode as u8) << 4;
        val |= (operating_mode as u8) << 1;
        unsafe {
            cmd.write(val);
        }
    }

    pub fn start_timer(&self, mode: OperatingMode, freq: u32) -> Result<(), TryFromIntError> {
        let mut pit = self.0.lock();
        let divisor: u16 = (TIMER_FREQUENCY / freq).try_into()?;

        Self::set_cmd(&mut pit.cmd, Channel::Channel0, AccessMode::LoHiByte, mode);
        unsafe {
            pit.ch.write((divisor & 0xff) as u8);
            pit.ch.write((divisor >> 8) as u8);
        }
        Ok(())
    }

    pub fn get_count(&self) -> u16 {
        let mut pit = self.0.lock();
        unsafe {
            let lo = pit.ch.read();
            let hi = pit.ch.read();
            ((hi as u16) << 8) | (lo as u16)
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Channel {
    Channel0 = 0,
    Channel1 = 1,
    Channel2 = 2,
}

impl Channel {
    pub const fn port(&self) -> u16 {
        match self {
            Self::Channel0 => 0x40,
            Self::Channel1 => 0x41,
            Self::Channel2 => 0x42,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum AccessMode {
    LatchCountValue = 0,
    LoByte = 1,
    HiByte = 2,
    LoHiByte = 3,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum OperatingMode {
    InterruptOnTerminalCount = 0,
    HardwareRetriggerableOneShot = 1,
    RateGenerator = 2,
    SquareWaveGenerator = 3,
    SoftwareTriggeredStrobe = 4,
    HardwareTriggeredStrobe = 5,
}
