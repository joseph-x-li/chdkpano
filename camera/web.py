from threading import local
import streamlit as st
import cv2
from ppmreader import parse_ppm
import numpy as np

st.title("Camera Array Controls")
def local_css(file_name):
  with open(file_name) as f:
    st.markdown(f'<style>{f.read()}</style>', unsafe_allow_html=True)

local_css("style.css")

col0, col1, col2, col3 = st.columns(4)


image = np.rot90(cv2.resize(parse_ppm('../ppmtest.ppm'), (1600, 900)))
status_image = np.zeros((20, 100, 3), np.uint8)
status_image[:, :, 0] = 255


with col0:
  st.image(status_image, use_column_width=True)
  placeholder = st.empty()
  placeholder.image(image, use_column_width=True, caption="Cam_0")
  
with col1:
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True, caption="Cam_1")
  
with col2:
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True, caption="Cam_2")
  
with col3:
  st.image(status_image, use_column_width=True)
  st.image(image, use_column_width=True, caption="Cam_3")

l_col, r_col = st.columns([7, 3])

with l_col:
  st.select_slider("Exposure", ["Auto", "Stack"], "Auto")
  st.select_slider("FOV", ["Full", "Fit"], "Full")
  st.select_slider("Zoom", ["1x"] + [f"{x}x" for x in range(2, 10, 2)], "1x")

with r_col:
  st.selectbox("Delay", [f"{x}s" for x in range(11)], index=0)
  st.selectbox("Viewfinder Refresh", ["0.5s", "1s", "2s", "3s", "4s", "OFF"], index=2)
  st.button("SHOOT", on_click=lambda: st.warning("Shoot!"))
