from adafruit_servokit import ServoKit
import time

class ServoArray:
  reset_angle = [120, 120, 40, 40]
  power_angle = [180, 180, 90, 90]
  battery_angle = [50, 50, 0, 0]
  
  safe_delay = 0.25

  def __init__(self):
    self.kit = ServoKit(channels=16)
  
  def reset(self):
    for i in range(4):
      self.kit.servo[i].angle = self.reset_angle[i]
      time.sleep(self.safe_delay)
      self.kit.servo[i].angle = None
  
  def power_position(self):
    for i in range(4):
      self.kit.servo[i].angle = self.power_angle[i]
      time.sleep(self.safe_delay)
      self.kit.servo[i].angle = None

  def set_position(self, angles):
    for i, angle in zip(range(4), angles):
      self.kit.servo[i].angle = angle
      time.sleep(self.safe_delay)
      self.kit.servo[i].angle = None



if __name__ == "__main__":
  servo = ServoArray()
  input()
  servo.power_position()
  input()
  servo.reset()
  input()
  servo.battery_position()
  input()
  servo.reset()
