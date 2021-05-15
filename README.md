# image-decompose

The tool decomposes an RGB image into it’s channels in different
colour spaces.  sRGB (including linear RGB), HSL, HSV, HBW, XYZ, xyY,
L\*a\*b\*, LCh<sub>ab</sub>, L\*u\*v\*, LCH<sub>uv</sub>, CMY and CMYK
models are supported.

For each of those the program will load input image as an sRGB image,
convert it to given colour space and then create an image which
includes coordinates

# Example

An example image is included in `data` directory which can be used to
test the program:

    cargo run -- -f --resize 256x256 --crop 200x200+28+28 \
                 -o out data/lenna.png

As a result, the tool generates handful of WebP images and saves them
in the `out` directory with names matching `lenna-*.webp` pattern.
Each of the image is for specific colour space.

## sRGB

![Decomposition of the Lenna test image into red, green and blue channels](out/lenna-rgb.webp)

## HSL

![Decomposition of the Lenna test image into hue, saturaiton and
lightens channels of HSL model](out/lenna-hsl.webp)

## L\*u\*v\*

![Decomposition of the Lenna test image into L\*, u\* and v\* channels](out/lenna-luv.webp)

## LCh<sub>uv</sub>

![Decomposition of the Lenna test image into L\*, C\* and hue channels
of LCh(uv) model](out/lenna-lchuv.webp)

## CMY

![Decomposition of the Lenna test image into cyan, magenta and yellow
channels](out/lenna-cmy.webp)
