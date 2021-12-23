from threading import local
import streamlit as st
import cv2
from ppmreader import parse_ppm
import numpy as np

# st.title("Camera Array Controls")
def local_css(file_name):
  with open(file_name) as f:
    st.markdown(f'<style>{f.read()}</style>', unsafe_allow_html=True)

local_css("style.css")

col0, col1, col2, col3 = st.columns(4)


image = np.rot90(cv2.resize(parse_ppm('../ppmtest.ppm'), (1600, 900)))
status_image = np.zeros((20, 100, 3), np.uint8)
status_image[:, :, 0] = 255

with col0:
  st.text("Cam_0")
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True)
  
with col1:
  st.text("Cam_1")
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True)
  
with col2:
  st.text("Cam_2")
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True)
  
with col3:
  st.text("Cam_3")
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True)

slider_col, button_col = st.columns([8, 2])

with slider_col:
  st.select_slider("Delay", [f"{x}s" for x in range(11)], "0s")
  st.select_slider("Exposure", ["Auto", "Stack"], "Auto")
  st.select_slider("Zoom", [f"{x}x" for x in range(0, 10, 2)], "0x")

with button_col:
  st.button("SHOOT")