#!/bin/bash

# Ensure a file name is provided
if [ "$#" -ne 1 ]; then
    echo "Usage: $0 <file>"
    exit 1
fi

file="$1"

# Use sed to:
# 1) Remove the first two dots from dates.
# 2) Replace the first comma with a space to separate date and time.
# 3) Remove all colons from times.
# 4) Correctly format the time by adding "00" for milliseconds directly after the time (before the first semicolon).
# 5) Replace all commas with semicolons.
# Then, remove the first semicolon from each line.
sed -E 's/([0-9]{4})\.([0-9]{2})\.([0-9]{2}),/\1\2\3 /; s/://g; s/([0-9]{2})([0-9]{2}),/\1\2;00;/; s/,/;/g' "$file" | sed -E 's/([0-9]{2})([0-9]{2}) ([0-9]{2})([0-9]{2}),/\1\2\3\4;00;/g' | sed 's/;//' > "${file%.csv}_modified.csv"

echo "File transformed and saved as ${file%.csv}_modified.csv"

