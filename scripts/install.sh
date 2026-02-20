#!/bin/bash
set -e
echo "Installing BarqCoder..."
curl -L https://github.com/YASSERRMD/barq-coder/releases/latest/download/barqcoder -o /usr/local/bin/barqcoder
chmod +x /usr/local/bin/barqcoder
echo "BarqCoder installed successfully!"
barqcoder --version || true
