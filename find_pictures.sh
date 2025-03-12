#/bin/sh

find ~/Pictures/ -name "*.png" -print0 | while IFS= read -r -d '' file; do  
    echo "$file"; 
    cargo run --bin  astrohud-client "$file"; 
    sleep .1; 
done