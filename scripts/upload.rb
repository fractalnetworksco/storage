#!/usr/bin/env ruby
require 'net/http'
require 'ed25519'

privkey = Ed25519::SigningKey.generate
pubkey = privkey.verify_key
pubkey_hex = pubkey.to_bytes.unpack("H*").first

puts "Pubkey: #{pubkey_hex}"

time = Time.now.to_i
header = [
  1234, # generation
  0,    # parent (zero means none)
  time, # timestamp
]
puts "Header {"
puts "  generation: #{header[0]}"
puts "  parent: #{header[1]}"
puts "  time: #{header[2]}"
puts "}"

header = header.pack("Q>Q>Q>")
data = "Somerandomdata"

puts "Data: #{data}"

# compute signature
signature = privkey.sign(header + data)
puts "Signature: #{signature.unpack("H*").first}"

# body is header + data + signature
body = header + data + signature

http = Net::HTTP.new("localhost", 8002)
#http.set_debug_output $stderr
response = http.request_post("/snapshot/#{pubkey_hex}/create", "")
puts response
response = http.request_post("/snapshot/#{pubkey_hex}/upload", body)
puts response
#response =  http.request_get("/snapshot/#{pubkey_hex}/latest")
#puts response
#puts response.body
response = http.request_get("/snapshot/#{pubkey_hex}/fetch?generation=1234")
puts response
p response.body
