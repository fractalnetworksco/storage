#!/usr/bin/env ruby
require 'net/http'
require 'ed25519'

privkey = Ed25519::SigningKey.generate
pubkey = privkey.verify_key
pubkey_hex = pubkey.to_bytes.unpack("H*").first

puts "Pubkey: #{pubkey_hex}"
time = Time.now.to_i

def upload(http, privkey, data, generation, parent, time)
  pubkey = privkey.verify_key
  pubkey_hex = pubkey.to_bytes.unpack("H*").first
  header = [
    generation,
    parent,
    time,
  ].pack("Q>Q>Q>")
  signature = privkey.sign(header + data)
  body = header + data + signature
  http.request_post("/snapshot/#{pubkey_hex}/upload", body)
end

def create(http, privkey)
  pubkey = privkey.verify_key
  pubkey_hex = pubkey.to_bytes.unpack("H*").first
  http.request_post("/snapshot/#{pubkey_hex}/create", "")
end

def latest(http, privkey, parent = nil)
  pubkey = privkey.verify_key
  pubkey_hex = pubkey.to_bytes.unpack("H*").first
  if parent
    http.request_get("/snapshot/#{pubkey_hex}/latest?parent=#{parent}")
  else
    http.request_get("/snapshot/#{pubkey_hex}/latest")
  end
end

def fetch(http, privkey, generation, parent = nil)
  pubkey = privkey.verify_key
  pubkey_hex = pubkey.to_bytes.unpack("H*").first
  if parent
    http.request_get("/snapshot/#{pubkey_hex}/fetch?generation=#{generation}&parent=#{parent}")
  else
    http.request_get("/snapshot/#{pubkey_hex}/fetch?generation=#{generation}")
  end
end

def list(http, privkey, genmin = nil, genmax = nil, parent = nil)
  pubkey = privkey.verify_key
  pubkey_hex = pubkey.to_bytes.unpack("H*").first
  url = "/snapshot/#{pubkey_hex}/list"
  query = []
  if genmin
    query << "genmin=#{genmin}"
  end
  if genmax
    query << "genmax=#{genmax}"
  end
  if parent
    query << "parent=#{parent}"
  end
  query = query.join("&")
  if query.length > 0
    url += "?" + query
  end
  http.request_get(url)
end

http = Net::HTTP.new("localhost", 8002)
puts latest(http, privkey).body
puts create(http, privkey)
#puts latest(http, privkey).body
puts upload(http, privkey, "Somerandomdata", 1234, 0, Time.now.to_i)
puts latest(http, privkey).body
puts upload(http, privkey, "Otherrandomdata", 1235, 1234, Time.now.to_i)
puts latest(http, privkey).body
puts latest(http, privkey, 1234).body
puts fetch(http, privkey, 1234).body[24..-65]
puts fetch(http, privkey, 1235, 1234).body[24..-65]
puts list(http, privkey).body

