use core::fmt::Write;

use spin::{Lazy, Mutex};
use x86_64::instructions::port::{PortRead, PortWrite};

pub static COM1: Lazy<Mutex<Serial>> = Lazy::new(|| {
    let Ok(serial) = Serial::com1() else {
        crate::panic::halt_and_never_return();
    };
    Mutex::new(serial)
});

#[macro_export]
macro_rules! kprint {
    ($($args:tt)*) => {
        {
            use ::core::fmt::Write;
            let mut serial = $crate::serial::COM1.lock();
            // Serial write will never fail
            let _ = write!(*serial, $($args)*);
        }
    };
}

#[macro_export]
macro_rules! kprintln {
    ($($args:tt)*) => {
        {
            use ::core::fmt::Write;
            let mut serial = $crate::serial::COM1.lock();
            // Serial write will never fail
            // Use write! instead of writeln! to ensure a carriage return is written
            let _ = write!(*serial, $($args)*);
            serial.write_byte(b'\r');
            serial.write_byte(b'\n');
        }
    };
}

pub struct Serial {
    port: u16,
}

impl Serial {
    const COM1: u16 = 0x3F8;

    pub fn com1() -> Result<Self, SerialError> {
        unsafe { Self::new(Self::COM1) }
    }

    pub unsafe fn new(port: u16) -> Result<Self, SerialError> {
        Self::init_serial(port)?;
        Ok(Self { port })
    }

    fn init_serial(port: u16) -> Result<(), SerialError> {
        unsafe {
            u8::write_to_port(port + 1, 0x00); // Disable all interrupts
            u8::write_to_port(port + 3, 0x80); // Enable DLAB (set baud rate divisor)
            u8::write_to_port(port, 0x03); // Set divisor to 3 (lo byte) 38400 baud
            u8::write_to_port(port + 1, 0x00); //                  (hi byte)
            u8::write_to_port(port + 3, 0x03); // 8 bits, no parity, one stop bit
            u8::write_to_port(port + 2, 0xC7); // Enable FIFO, clear them, with 14-byte threshold
            u8::write_to_port(port + 4, 0x0B); // IRQs enabled, RTS/DSR set
            u8::write_to_port(port + 4, 0x1E); // Set in loopback mode, test the serial chip
            u8::write_to_port(port, 0xAE); // Test serial chip (send byte 0xAE and check if serial returns same byte)

            // Check if serial is faulty (i.e: not same byte as sent)
            if u8::read_from_port(port) != 0xAE {
                return Err(SerialError);
            }

            // If serial is not faulty set it in normal operation mode
            // (not-loopback with IRQs enabled and OUT#1 and OUT#2 bits enabled)
            u8::write_to_port(port + 4, 0x0F);
            Ok(())
        }
    }

    pub fn enable_interrupts(&mut self) {
        unsafe {
            u8::write_to_port(self.port + 1, 0x01);

            // Acknowledge any pending interrupts
            u8::read_from_port(self.port + 2);
            u8::read_from_port(self.port);
        }

        // Enable interrupts on IOAPIC
        let mut ioapic = crate::apic::IOAPIC.lock();
        ioapic.enable(crate::trap::IRQ_COM1, 0);
    }

    pub fn write_byte(&mut self, byte: u8) {
        unsafe {
            u8::write_to_port(self.port, byte);
        }
    }

    pub fn data_available(&mut self) -> bool {
        unsafe { u8::read_from_port(self.port + 5) & 1 == 1 }
    }

    pub fn read_byte(&mut self) -> Option<u8> {
        if self.data_available() {
            Some(unsafe { u8::read_from_port(self.port) })
        } else {
            None
        }
    }

    pub fn handle_interrupt(&mut self) {
        while let Some(byte) = self.read_byte() {
            match byte {
                // Backspace
                0x7f => {
                    self.write_byte(b'\x08');
                    self.write_byte(b' ');
                    self.write_byte(b'\x08');
                }
                // New line
                b'\r' | b'\n' => {
                    self.write_byte(b'\r');
                    self.write_byte(b'\n');
                }
                b => self.write_byte(b),
            }
        }
    }
}

impl Write for Serial {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            self.write_byte(byte);
        }
        Ok(())
    }
}

pub struct SerialError;

impl core::fmt::Debug for SerialError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str("SerialError")
    }
}
