#!/bin/sh
# Oracle for openssl-issuing-ca-chain: request, then sign, twice -- and the
# intermediate is given the basic constraints that make it a CA at all,
# because signing alone does not confer that.
set -eu
cd "${1:-.}"

openssl req -new -key issuing.key -sha256 \
  -subj '/O=Harbour Works/CN=Harbour Works Issuing CA' -out issuing.csr
printf '%s\n' \
  '[issuing_ca]' \
  'basicConstraints=critical,CA:TRUE,pathlen:0' \
  'keyUsage=critical,keyCertSign,cRLSign' \
  'subjectKeyIdentifier=hash' > issuing.ext
openssl x509 -req -in issuing.csr -CA root.crt -CAkey root.key \
  -days 1825 -sha256 -extfile issuing.ext -extensions issuing_ca -out issuing.crt

openssl req -new -key db.key -sha256 \
  -subj '/O=Harbour Works/CN=db.internal' -out db.csr
printf '%s\n' \
  '[host]' \
  'basicConstraints=critical,CA:FALSE' \
  'keyUsage=critical,digitalSignature,keyEncipherment' \
  'extendedKeyUsage=serverAuth' \
  'subjectAltName=DNS:db.internal' > db.ext
openssl x509 -req -in db.csr -CA issuing.crt -CAkey issuing.key \
  -days 365 -sha256 -extfile db.ext -extensions host -out db.crt

openssl verify -CAfile root.crt -untrusted issuing.crt db.crt
