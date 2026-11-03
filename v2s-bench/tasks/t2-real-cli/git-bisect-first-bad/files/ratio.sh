#!/bin/sh
# ratio.sh A B -- print A:B reduced by their greatest common divisor.
a=$1
b=$2
g=$a
h=$b
while [ "$h" -ne 0 ]; do
  t=$((g % h))
  g=$h
  h=$t
done
echo "$((a / g)):$((b / g))"
