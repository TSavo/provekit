#!/bin/bash
while IFS= read -r line; do
  method=$(echo "$line" | grep -o '"method":"[^"]*"' | cut -d'"' -f4)
  id=$(echo "$line" | grep -o '"id":[0-9]*' | cut -d':' -f2)
  if [ "$method" = "initialize" ]; then
    echo '{"jsonrpc":"2.0","id":'$id',"result":{"name":"go-lift","version":"1.0"}}'
  elif [ "$method" = "lift" ]; then
    echo '{"jsonrpc":"2.0","id":'$id',"result":{"kind":"proof-envelope","filename_cid":"blake3-512:go123","bytes_base64":"Z28="}}'
  elif [ "$method" = "shutdown" ]; then
    echo '{"jsonrpc":"2.0","id":'$id',"result":null}'
    exit 0
  fi
done
