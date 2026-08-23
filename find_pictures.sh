#/bin/sh

DIR="/media/sbeskur/Seagate Backup Plus Drive/Photo Library/BMX"


find "$DIR" -type f \( -iname "*.png" -o -iname "*.jpg" -o -iname "*.jpeg" -o -iname "*.gif" -o -iname "*.bmp" \) -print0 | while IFS= read -r -d '' file; do
    echo "$file"; 
    cargo run --bin  astrohud-client $1 "$file"; 
    sleep .2; 
done        

DIR="/media/sbeskur/Seagate Backup Plus Drive/Photo Library/Trip to Parkes"

find "$DIR" -type f \( -iname "*.png" -o -iname "*.jpg" -o -iname "*.jpeg" -o -iname "*.gif" -o -iname "*.bmp" \) -print0 | while IFS= read -r -d '' file; do
    echo "$file"; 
    cargo run --bin  astrohud-client $1 "$file"; 
    sleep .2; 
done        