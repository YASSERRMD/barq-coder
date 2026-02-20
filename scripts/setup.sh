#!/bin/bash
set -e
echo "Setting up BarqCoder environment..."
docker-compose up -d ollama
echo "Waiting for Ollama to boot..."
sleep 5
echo "Pulling models..."
docker-compose exec ollama ollama pull qwen2.5-coder:1.5b
echo "Setup complete!"
