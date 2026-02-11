#!/bin/bash
# Convert cargo-built ELF to esptool-flashable binary
# Usage: cargo_elf_to_esptool.sh <elf-path> <output-bin>

set -e

if [ $# -lt 2 ]; then
    echo "Usage: $0 <elf-path> <output-bin>"
    echo "Example: $0 ../badge-tester/target/xtensa-esp32s3-none-elf/release/badge-tester firmware.bin"
    exit 1
fi

ELF="$1"
OUTPUT="$2"

if [ ! -f "$ELF" ]; then
    echo "Error: ELF file not found: $ELF"
    exit 1
fi

# Check if espflash is available
if ! command -v espflash &> /dev/null; then
    echo "Error: espflash not found. Install with:"
    echo "  cargo install espflash"
    exit 1
fi

echo "Converting $ELF to esptool-compatible binary..."

# Use espflash to save a merged flash image
# This generates a single binary with bootloader, partition table, and app
espflash save-image --chip esp32s3 --merge "$ELF" "$OUTPUT"

SIZE=$(du -h "$OUTPUT" | cut -f1)
echo "✓ Created $OUTPUT ($SIZE)"
echo ""
echo "Flash with esptool:"
echo "  python -m esptool --chip esp32s3 write_flash 0x0 $OUTPUT"
