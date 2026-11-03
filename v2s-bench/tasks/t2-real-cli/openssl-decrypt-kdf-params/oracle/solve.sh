#!/bin/sh
# Oracle for openssl-decrypt-kdf-params: every key-derivation choice the
# seal made is restated, including the digest, which is not the default.
set -eu
cd "${1:-.}"
openssl enc -d -aes-256-cbc -pbkdf2 -iter 240000 -md sha512 \
  -in vault.enc -out vault.txt -pass file:passphrase.txt
openssl dgst -sha256 -r vault.txt
cat vault.sha256
