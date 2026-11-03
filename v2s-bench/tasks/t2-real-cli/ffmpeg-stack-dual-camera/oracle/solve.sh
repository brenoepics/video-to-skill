#!/bin/sh
# Oracle for ffmpeg-stack-dual-camera: the real graph, driven through ffmpeg.
# The labelled graph output and the chosen audio stream are both mapped
# explicitly, and the output is told to end with the stream that ends first.
set -eu
cd "${1:-.}"
ffmpeg -nostdin -v error -y \
  -i cam-left.mp4 -i cam-right.mp4 \
  -filter_complex "[0:v][1:v]hstack=inputs=2[stacked]" \
  -map "[stacked]" -map 1:a \
  -shortest \
  review.mp4
ffprobe -v error -show_entries format=duration:stream=index,codec_type,width,height \
  -of default=noprint_wrappers=1 review.mp4
