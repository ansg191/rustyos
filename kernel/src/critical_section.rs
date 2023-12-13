use critical_section::RawRestoreState;
use x86_64::instructions::interrupts;

struct CriticalSection;
critical_section::set_impl!(CriticalSection);

unsafe impl critical_section::Impl for CriticalSection {
    unsafe fn acquire() -> RawRestoreState {
        let restore_state = interrupts::are_enabled();

        if restore_state {
            interrupts::disable();
        }

        restore_state
    }

    unsafe fn release(restore_state: RawRestoreState) {
        if restore_state {
            interrupts::enable();
        }
    }
}
