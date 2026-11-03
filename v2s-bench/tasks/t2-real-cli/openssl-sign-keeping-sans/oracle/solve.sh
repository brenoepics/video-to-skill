#!/bin/sh
# Oracle for openssl-sign-keeping-sans: the signing step is told to take
# over what the request asked for, which is not the default.
set -eu
cd "${1:-.}"
openssl x509 -req -in api.csr -CA ca.crt -CAkey ca.key \
  -days 365 -sha256 -copy_extensions copy -out api.crt
openssl x509 -in api.crt -noout -ext subjectAltName,keyUsage,extendedKeyUsage
openssl verify -CAfile ca.crt api.crt
