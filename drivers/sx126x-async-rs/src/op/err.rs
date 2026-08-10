//! Device error and fault reporting structures.

/// A bitmask representing the error flags from the device.
#[derive(Copy, Clone)]
pub struct DeviceErrors {
    inner: u16,
}

impl core::fmt::Debug for DeviceErrors {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeviceErrors")
            .field("rc64k_calib_err", &self.rc64k_calib_err())
            .field("rc13m_calib_err", &self.rc13m_calib_err())
            .field("pll_calib_err", &self.pll_calib_err())
            .field("adc_calib_err", &self.adc_calib_err())
            .field("img_calib_err", &self.img_calib_err())
            .field("xosc_start_err", &self.xosc_start_err())
            .field("pll_lock_err", &self.pll_lock_err())
            .field("pa_ramp_err", &self.pa_ramp_err())
            .finish()
    }
}

impl From<u16> for DeviceErrors {
    fn from(val: u16) -> Self {
        Self { inner: val }
    }
}

impl DeviceErrors {
    /// RC64K calibration error.
    pub fn rc64k_calib_err(self) -> bool {
        (self.inner & 1 << 0) > 0
    }

    /// RC13M calibration error.
    pub fn rc13m_calib_err(self) -> bool {
        (self.inner & 1 << 1) > 0
    }

    /// PLL calibration error.
    pub fn pll_calib_err(self) -> bool {
        (self.inner & 1 << 2) > 0
    }

    /// ADC calibration error.
    pub fn adc_calib_err(self) -> bool {
        (self.inner & 1 << 3) > 0
    }

    /// Image calibration error.
    pub fn img_calib_err(self) -> bool {
        (self.inner & 1 << 4) > 0
    }

    /// XOSC start error.
    pub fn xosc_start_err(self) -> bool {
        (self.inner & 1 << 5) > 0
    }

    /// PLL lock error.
    pub fn pll_lock_err(self) -> bool {
        (self.inner & 1 << 6) > 0
    }

    /// PA ramping error.
    pub fn pa_ramp_err(self) -> bool {
        (self.inner & 1 << 8) > 0
    }
}
