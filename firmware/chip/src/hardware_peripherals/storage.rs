use core::sync::atomic::{AtomicU8, Ordering};

use embassy_sync::blocking_mutex::NoopMutex as NoopBlockingMutex;
use embedded_hal::delay::DelayNs;
use embedded_storage::{ReadStorage, Storage};
use esp_hal::system::CpuControl;
use esp_storage::{FlashStorage, FlashStorageError};

pub const DMA_FLASH_STATE_IDLE: u8 = 0;
pub const DMA_FLASH_STATE_DMA_ACTIVE: u8 = 1;
pub const DMA_FLASH_STATE_FLASH_WRITE: u8 = 2;

pub static DMA_FLASH_STATE: AtomicU8 = AtomicU8::new(DMA_FLASH_STATE_IDLE);

pub struct EspStorageInternals<'a> {
    pub flash_storage: FlashStorage<'a>,
    pub cpu_control: CpuControl<'a>,
}

/// This struct exists because we need access to both the FlashStorage and CpuControl
/// in order to write to flash.  Keeping the internals in a mutex allows us to create and
/// pass around as many EspStorage objects as we want even though their internals are
/// singletons -- it also avoids any potential deadlocks caused by them being in two
/// separate mutexes.
impl<'a> EspStorageInternals<'a> {
    pub fn new(flash_storage: FlashStorage<'a>, cpu_control: CpuControl<'a>) -> Self {
        Self {
            flash_storage,
            cpu_control,
        }
    }
}

pub struct EspStorage<'a> {
    internals: &'a NoopBlockingMutex<EspStorageInternals<'a>>,
    delay: esp_hal::delay::Delay,
}

impl<'a> EspStorage<'a> {
    pub fn new(internals: &'a NoopBlockingMutex<EspStorageInternals<'a>>) -> Self {
        Self {
            internals,
            delay: esp_hal::delay::Delay::new(),
        }
    }
}

impl ReadStorage for EspStorage<'_> {
    type Error = FlashStorageError;

    // Needs to be unsafe because lock_mut is unsafe.  However, we don't lock it
    // reentrantly, which is the specific behavior we need to avoid.
    #[allow(unsafe_code)]
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), FlashStorageError> {
        unsafe {
            self.internals
                .lock_mut(|guard| guard.flash_storage.read(offset, bytes))
        }
    }

    fn capacity(&self) -> usize {
        self.internals.lock(|guard| guard.flash_storage.capacity())
    }
}

impl Storage for EspStorage<'_> {
    // This function is unsafe because parking a core is unsafe
    // (if you park the core this is running on, it's not good)
    // But, we need to do that to avoid deadlocks when writing to flash.
    // We only write to flash from ProCpu (CPU0), since rendering the UI is
    // the only thing done on AppCpu (CPU1) and that doesn't require flash writes.
    #[allow(unsafe_code)]
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), FlashStorageError> {
        loop {
            match DMA_FLASH_STATE.compare_exchange(
                DMA_FLASH_STATE_IDLE,
                DMA_FLASH_STATE_FLASH_WRITE,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(_) => {
                    self.delay.delay_us(1);
                }
            }
        }
        // Parking inside a critical section guarantees Core 1 is not holding the
        // global spinlock when it stalls (Core 0 owns it, so Core 1 is either not
        // using it or spinning waiting for it).  The critical section is released
        // before the flash write so esp-storage can acquire it internally.
        //
        // DO NOT log or acquire the global spinlock between park and unpark --
        // Core 1 may be blocked waiting on the spinlock when it resumes, and
        // acquiring it here before unpark would deadlock.
        let flash_res = unsafe {
            self.internals.lock_mut(|guard| {
                critical_section::with(|_| {
                    log::debug!("Parking Core 1");
                    guard.cpu_control.park_core(esp_hal::system::Cpu::AppCpu);
                    loop {
                        if !(esp_hal::system::is_running(esp_hal::system::Cpu::AppCpu)) {
                            break;
                        }
                    }
                });
                let res = guard.flash_storage.write(offset, bytes);
                guard.cpu_control.unpark_core(esp_hal::system::Cpu::AppCpu);
                log::debug!("Unparked Core 1");
                res
            })
        };

        DMA_FLASH_STATE.store(DMA_FLASH_STATE_IDLE, Ordering::Release);
        flash_res
    }
}
