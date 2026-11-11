#!/usr/bin/env python3
"""
Test VinBus API encryption based on decompiled app (er2.m()).
Encryption scheme:
1. Build payload: [random_int(4 bytes)] + [data_bytes]
2. Generate random key1 (10-15 bytes) and key2 (10-15 bytes)
3. Build bArr2 = key1 XOR key2, extended to max length
4. Output format:
   - byte 0: key1_len
   - bytes 1-2: data_length (little endian)
   - byte 3: key2_len
   - bytes 4..4+key1_len-1: key1
   - bytes 4+key1_len..4+key1_len+key2_len-1: key2
   - remaining bytes: encrypted data XORed with bArr2 cyclically
5. Base64 encode
"""
import base64
import random
import struct
import requests

BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

def build_encrypted_payload():
    # Build payload: [random_int(4 bytes)] + [data_bytes]
    # From decompiled app: 4 floats (origin_lat, origin_lng, dest_lat, dest_lng) + 1 byte (value 2)
    payload_data = bytearray()
    
    # Random int (4 bytes)
    rand_int = random.randint(0, 0xFFFFFFFF)
    payload_data.extend(struct.pack('<i', rand_int))
    
    # Route data: 4 floats + 1 byte value 2
    payload_data.extend(struct.pack('<ffff', 21.0285, 105.8542, 21.0385, 105.8642))  # Hanoi coords
    payload_data.append(2)  # value 2
    
    # Generate random keys
    key1_len = random.randint(10, 15)
    key2_len = random.randint(10, 15)
    key1 = bytes([random.randint(48, 127) for _ in range(key1_len)])
    key2 = bytes([random.randint(48, 127) for _ in range(key2_len)])
    
    # Build bArr2 = key1 XOR key2, extended to max length
    max_key_len = max(key1_len, key2_len)
    b_arr2 = bytearray(max_key_len)
    for i in range(max_key_len):
        k1 = key1[i] if i < key1_len else 0
        k2 = key2[i] if i < key2_len else 0
        b_arr2[i] = k1 ^ k2
    
    # XOR payload data with b_arr2 cyclically
    encrypted_data = bytearray(len(payload_data))
    for i in range(len(payload_data)):
        encrypted_data[i] = payload_data[i] ^ b_arr2[i % len(b_arr2)]
    
    # Build output buffer:
    # byte 0: key1_len
    # bytes 1-2: data_length (little endian)
    # byte 3: key2_len
    # bytes 4..4+key1_len-1: key1
    # bytes 4+key1_len..4+key1_len+key2_len-1: key2
    # remaining bytes: encrypted data XORed with bArr2 cyclically
    output = bytearray()
    output.append(key1_len)
    output.append(payload_data.len() & 0xFF)
    output.append((payload_data.len() >> 8) & 0xFF)
    output.append(key2_len)
    output.extend(key1)
    output.extend(key2)
    output.extend(encrypted_data)
    
    # Base64 encode
    return base64.b64encode(bytes(output)).decode('utf-8')

def call_api(endpoint, encrypted_payload):
    url = f"{BASE_URL}/{endpoint}?ReqParams={encrypted_payload}"
    try:
        resp = requests.get(url, timeout=15)
        print(f"  HTTP Status: {resp.status_code}")
        print(f"  Response: {resp.text[:200]}")
        return resp
    except Exception as e:
        print(f"Error: {e}")
        return None

# Test 1: Route search
print("=== Test 1: Route search (RouteBuytOptionV3) ===")
payload = build_encrypted_payload()
print(f"Encrypted payload: {encrypted[:100]}...")
resp = call_api("RouteBuytOptionV3", encrypted)

# Test 2: Vehicle state for bus 35B
print("\n--- Test 2: Vehicle state (VehicleState) ---")
payload2 = build_encrypted_payload()  # Same format
resp2 = call_api("VehicleState", encrypted2)
