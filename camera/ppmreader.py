import numpy as np

def parse_ppm(filename):
    with open(filename, 'r') as f:
        lines = f.readlines()
    
    cols, rows = lines[2].split(' ')
    cols, rows = int(cols), int(rows)
    
    accum = []
    for line in lines[4:]:
        accum += line.split(' ')
    
    accum = [int(x) for x in accum if x]
    
    return np.array(accum).astype(np.uint8).reshape((rows, cols, 3))

if __name__ == '__main__':
    img = parse_ppm('ppmtest.ppm')
    import cv2
    cv2.imwrite('hi.png', cv2.cvtColor(img, cv2.COLOR_RGB2BGR))