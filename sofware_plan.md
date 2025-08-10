Three components

## Component 1: Servo-driving
 - Basically to write a small rust library for controlling the four servos. They are all hooked up to the standard 16-Channel PWM Servo Controller Module. I want the functionality of passing four angles and all the servos go to that angle.
 - https://docs.rs/pwm-pca9685/latest/pwm_pca9685/

## Component 2: Poor interface to CHDKPTP
 - Create bindings to chdkptp in rust directly, coding against https://app.assembla.com/spaces/chdk/subversion/source/HEAD/trunk/core/ptp.h

# Component 3: Web server
 - Serves an interface for...
   - Controlling servo positions
   - Setting focuses
