from adafruit_servokit import ServoKit




class ServoArray:
    def __init__(self):
       pass 


kit = ServoKit(channels=16)
kit.servo[0].angle = 0

import time; time.sleep(1)
kit.servo[1].angle = 120
time.sleep(1)
kit.servo[0].angle = None
time.sleep(0.5)
