from adafruit_servokit import ServoKit
import time

class ServoArray:
  reset_angle = [120, 120, 60, 60]
  def __init__(self):
    self.kit = ServoKit(channels=16)
  
  def reset(self):
    for i in range(4):
      self.kit.servo[i].angle = self.reset_angle[i]
    time.sleep(0.20)
    for i in range(4):
      self.kit.servo[i].angle = None


if __name__ == "__main__":
  servo = ServoArray()
  servo.reset()
