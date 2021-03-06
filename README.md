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

    cargo run -- -y --resize 300x400 --crop 150x300+75+50 \
                 -o out data/umbrella-sky.jpg


As a result, the tool generates handful of WebP images and saves them
in the `out` directory with names matching `umbrella-sky-*.webp`
pattern.  Each of the image includes decomposition of the source image
into separate channels in given colour space.

For example:

## sRGB

![An photo with its decomposition into red, green and blue
channels](out/umbrella-sky-rgb.webp)

Perhaps the most familiar decomposition showing how much red, green
and blue is in each pixel of the image.  RGB model is additive thus
the result comes from adding all those colours.

## HSL

![An photo with its decomposition into hue, saturaiton and lightens
channels of HSL model](out/umbrella-sky-hsl.webp)

HSL attempts to be more user friendly by introducing more natural hue,
saturation and lightness controls.  The model isn’t perceptually
uniform though so changing only hue affects luminosity of the colour.

Black spots in the hue channel indicates grey colours (which includes
white and black) in the source images for which hue is undefined.

## L\*u\*v\* and LCh<sub>uv</sub>

![An photo with its decomposition into L\*, u\* and v\*
channels](out/umbrella-sky-luv.webp)

![An photo with its decomposition into L\*, C\* and hue channels of
LCh(uv) model](out/umbrella-sky-lchuv.webp)

L\*u\*v\* colour space tries to be perceptually uniform.  The
decomposition demonstrates the L\* channel corresponds to luminosity
while u\* and v\* coordinates fall on the green-red and blue-yellow
axes.

The L\*C\*h model makes the model easier to interpret by representing
chromaticity with more familiar hue and chroma values.

## CMY and CMYK

![An photo with its decomposition into cyan, magenta and yellow
channels](out/umbrella-sky-cmy.webp)

![An photo with its decomposition into cyan, magenta, yellow and black
channels](out/umbrella-sky-cmyk.webp)

CMY and CMYK colour models are subtractive.  This is demonstrated by
the channels being ‘inverses’ of the image.  The less red the image
has, the more cyan is used and the same for green-magenta and
blue-yellow pairs.  The inverse is especially apparent with black (or
key) channel in CMYK model.
