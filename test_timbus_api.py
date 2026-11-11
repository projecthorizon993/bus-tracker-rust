#!/usr/bin/env python3
"""
Test calling TimBus API for bus route 35B
Based on reverse engineering of TimBus APK v5.6.0
"""

import base64
import random
import struct
import requests
import hashlib

BASE_URL = "http://www.timbus.vn:9149/SearchBuyt"

def crc32(data):
    """Simple CRC32 checksum matching Java's CRC32"""
    import zlib
    return zlib.crc32(data) & 0xFF

def crypt(data_bytes):
    """
    Implement the encryption from ConnectionExecute.crypt():
    1. Generate two random byte arrays (keys)
    2. XOR data with first key
    3. Wrap with keys + checksum
    4. Base64 encode
    """
    data_len = len(data_bytes)
    
    # Generate key1: random length 32-64, random chars 48-127
    key1_len = random.randint(32, 64)
    key1 = bytes([random.randint(48, 127) for _ in range(key1_len)])
    
    # Generate key2: random length 32-64, random chars 48-127
    key2_len = random.randint(32, 64)
    key2 = bytes([random.randint(48, 127) for _ in range(key2_len)])
    
    # Step 1: XOR data with key1
    encrypted = bytearray(data_len)
    for i in range(data_len):
        encrypted[i] = data_bytes[i] ^ key1[i % key1_len]
    
    # Step 2: Build output buffer
    # Format: [encrypted_data][key1][key1_len_byte][key2][key2_len_byte][checksum_byte]
    buf = bytearray()
    buf.extend(encrypted)
    buf.extend(key1)
    buf.append(key1_len)  # byte length of key1
    buf.extend(key2)
    buf.append(key2_len)  # byte length of key2
    buf.append(crc32(bytes(buf)))  # checksum
    
    # Base64 encode
    return base64.b64encode(bytes(buf)).decode('utf-8')

def not_crypt(data_bytes):
    """Non-encrypted version from ConnectionExecute.notCrypt()"""
    buf = bytearray()
    # qh0.G(kr2.q()) - writes random int
    random_int = random.randint(0, 2**32 - 1)
    buf.extend(struct.pack('<I', random_int))
    buf.extend(data_bytes)
    buf.append(crc32(bytes(buf)))
    return base64.b64encode(bytes(buf)).decode('utf-8')

def build_route_request(origin_lat, origin_lng, dest_lat, dest_lng):
    """
    Build request for RouteBuytOptionV3 (route search between two points)
    Based on un2.n() in decompiled code:
    - 4 floats: origin_lat, origin_lng, dest_lat, dest_lng
    - 1 byte: value 2
    """
    buf = bytearray()
    buf.extend(struct.pack('<f', origin_lat))  # float
    buf.extend(struct.pack('<f', origin_lng))  # float
    buf.extend(struct.pack('<f', dest_lat))    # float
    buf.extend(struct.pack('<f', dest_lng))    # float
    buf.append(2)  # byte value 2
    return bytes(buf)

def build_vehicle_state_request(bus_id, route_no, plate_no, is_active=True):
    """
    Build request for VehicleState
    Based on n73.m():
    - int: bus ID
    - String: route number
    - String: plate number
    - boolean: active flag
    """
    buf = bytearray()
    buf.extend(struct.pack('<i', bus_id))  # int
    # Write string as length + bytes
    route_bytes = route_no.encode('utf-8')
    buf.extend(struct.pack('<H', len(route_bytes)))  # short length
    buf.extend(route_bytes)
    plate_bytes = plate_no.encode('utf-8')
    buf.extend(struct.pack('<H', len(plate_bytes)))  # short length
    buf.extend(plate_bytes)
    buf.append(1 if is_active else 0)  # boolean
    return bytes(buf)

def call_api(encrypted_payload):
    """Call the TimBus API with encrypted payload"""
    url = BASE_URL
    params = {'ReqParams': encrypted_payload}
    
    try:
        resp = requests.get(url, params=params, timeout=15)
        print(f"HTTP Status: {resp.status_code}")
        print(f"Response length: {len(resp.text)}")
        print(f"Response preview: {resp.text[:500]}")
        return resp
    except Exception as e:
        print(f"Error: {e}")
        return None

if __name__ == "__main__":
    print("=" * 60)
    print("Testing TimBus API for bus route 35B")
    print("=" * 60)
    
    # Test 1: Try route search with Hanoi coordinates
    print("\n--- Test 1: Route search (RouteBuytOptionV3) ---")
    print("Searching for route between Hanoi points...")
    payload = build_route_request(21.0285, 105.8542, 21.0385, 105.8642)
    encrypted = crypt(payload)
    print(f"Encrypted payload (first 100 chars): {encrypted[:100]}...")
    resp = call_api(encrypted)
    
    # Test 2: Try vehicle state search for bus 35B
    print("\n--- Test 2: Vehicle state (VehicleState) ---")
    print("Searching for bus 35B...")
    payload2 = build_vehicle_state_request(35, "35B", "50H41787")
    encrypted2 = crypt(payload2)
    print(f"Encrypted payload (first 100 chars): {encrypted2[:100]}...")
    resp2 = call_api(encrypted2)
