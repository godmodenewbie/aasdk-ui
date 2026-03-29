#!/bin/bash
set -e
# Generate Fake CA
openssl req -new -x509 -days 3650 -keyout fake_ca.key -out fake_ca.crt -nodes -subj "/C=US/ST=California/L=Mountain View/O=Google Automotive Link"

# Generate Client Key
openssl genrsa -out fake_client.key 2048

# Create CSR for Client
openssl req -new -key fake_client.key -out fake_client.csr -subj "/C=JP/ST=Tokyo/L=Hachioji/O=JVC Kenwood/OU=01"

# Sign Client CSR
cat > v3.ext <<'EOM'
basicConstraints=CA:FALSE
keyUsage = digitalSignature, nonRepudiation, keyEncipherment, dataEncipherment
extendedKeyUsage = clientAuth, serverAuth
EOM

openssl x509 -req -in fake_client.csr -CA fake_ca.crt -CAkey fake_ca.key -CAcreateserial -out fake_client.crt -days 3650 -extfile v3.ext
