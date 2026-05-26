use pwm_pca9685::{Pca9685, Channel};
use linux_embedded_hal::I2cdev;
use std::time::Duration;
use std::thread;

pub struct ServoArray {
    pca: Pca9685<I2cdev>,
    reset_angle: [u16; 4],
    power_angle: [u16; 4],
    battery_angle: [u16; 4],
    safe_delay: Duration,
}

impl ServoArray {
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        // Open I2C device (typically /dev/i2c-1 on Raspberry Pi)
        let i2c = I2cdev::new("/dev/i2c-1")?;
        
        // Create PCA9685 instance with default address (0x40)
        let mut pca = Pca9685::new(i2c, 0x40)?;
        
        // Set PWM frequency to 50Hz (typical for servos)
        pca.set_pwm_freq(50.0)?;
        
        // Initialize with default angles
        let reset_angle = [120, 120, 40, 40];
        let power_angle = [180, 180, 90, 90];
        let battery_angle = [50, 50, 0, 0];
        let safe_delay = Duration::from_millis(250);
        
        Ok(ServoArray {
            pca,
            reset_angle,
            power_angle,
            battery_angle,
            safe_delay,
        })
    }
    
    /// Convert angle (0-180) to PWM pulse width (typically 0.5ms to 2.5ms)
    fn angle_to_pulse_width(angle: u16) -> u16 {
        // Map 0-180 degrees to 0.5ms-2.5ms pulse width
        // PCA9685 operates at 4096 steps per period
        // At 50Hz, period is 20ms, so 0.5ms = 102 steps, 2.5ms = 512 steps
        let min_pulse = 102;  // 0.5ms
        let max_pulse = 512;  // 2.5ms
        
        if angle > 180 {
            return max_pulse;
        }
        
        let pulse_width = min_pulse + ((angle as f32 / 180.0) * (max_pulse - min_pulse) as f32) as u16;
        pulse_width
    }
    
    /// Set servo to specific angle
    fn set_servo_angle(&mut self, channel: Channel, angle: u16) -> Result<(), Box<dyn std::error::Error>> {
        let pulse_width = Self::angle_to_pulse_width(angle);
        self.pca.set_channel_on_off(channel, 0, pulse_width)?;
        Ok(())
    }
    
    /// Disable servo (set to None equivalent)
    fn disable_servo(&mut self, channel: Channel) -> Result<(), Box<dyn std::error::Error>> {
        self.pca.set_channel_on_off(channel, 0, 0)?;
        Ok(())
    }
    
    pub fn reset(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (i, &angle) in self.reset_angle.iter().enumerate() {
            let channel = Channel::try_from(i as u8)?;
            self.set_servo_angle(channel, angle)?;
            thread::sleep(self.safe_delay);
            self.disable_servo(channel)?;
        }
        Ok(())
    }
    
    pub fn power_position(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (i, &angle) in self.power_angle.iter().enumerate() {
            let channel = Channel::try_from(i as u8)?;
            self.set_servo_angle(channel, angle)?;
            thread::sleep(self.safe_delay);
            self.disable_servo(channel)?;
        }
        Ok(())
    }
    
    pub fn battery_position(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for (i, &angle) in self.battery_angle.iter().enumerate() {
            let channel = Channel::try_from(i as u8)?;
            self.set_servo_angle(channel, angle)?;
            thread::sleep(self.safe_delay);
            self.disable_servo(channel)?;
        }
        Ok(())
    }
    
    pub fn set_position(&mut self, angles: [u16; 4]) -> Result<(), Box<dyn std::error::Error>> {
        for (i, &angle) in angles.iter().enumerate() {
            let channel = Channel::try_from(i as u8)?;
            self.set_servo_angle(channel, angle)?;
            thread::sleep(self.safe_delay);
            self.disable_servo(channel)?;
        }
        Ok(())
    }
    
    /// Set custom angles for individual servos
    pub fn set_individual_position(&mut self, servo_index: usize, angle: u16) -> Result<(), Box<dyn std::error::Error>> {
        if servo_index >= 4 {
            return Err("Servo index out of range (0-3)".into());
        }
        
        let channel = Channel::try_from(servo_index as u8)?;
        self.set_servo_angle(channel, angle)?;
        thread::sleep(self.safe_delay);
        self.disable_servo(channel)?;
        Ok(())
    }
    
    /// Get current angle settings
    pub fn get_reset_angles(&self) -> [u16; 4] {
        self.reset_angle
    }
    
    pub fn get_power_angles(&self) -> [u16; 4] {
        self.power_angle
    }
    
    pub fn get_battery_angles(&self) -> [u16; 4] {
        self.battery_angle
    }
    
    /// Update preset angles
    pub fn update_reset_angles(&mut self, angles: [u16; 4]) {
        self.reset_angle = angles;
    }
    
    pub fn update_power_angles(&mut self, angles: [u16; 4]) {
        self.power_angle = angles;
    }
    
    pub fn update_battery_angles(&mut self, angles: [u16; 4]) {
        self.battery_angle = angles;
    }
}

impl Drop for ServoArray {
    fn drop(&mut self) {
        // Ensure all servos are disabled when the struct is dropped
        for i in 0..4 {
            if let Ok(channel) = Channel::try_from(i as u8) {
                let _ = self.disable_servo(channel);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_angle_to_pulse_width() {
        assert_eq!(ServoArray::angle_to_pulse_width(0), 102);
        assert_eq!(ServoArray::angle_to_pulse_width(90), 307);
        assert_eq!(ServoArray::angle_to_pulse_width(180), 512);
        assert_eq!(ServoArray::angle_to_pulse_width(200), 512); // Clamped to max
    }
    
    #[test]
    fn test_servo_array_creation() {
        // This test will fail on systems without I2C access
        // but it's useful for development
        let result = ServoArray::new();
        // We can't easily test this without hardware, so we just check it doesn't panic
        // In a real test environment, you'd mock the I2C interface
    }
}
