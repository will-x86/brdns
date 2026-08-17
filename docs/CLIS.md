# netcat 


Listen on port 1053, direct to txt
```
nc -u -l 1053 > query_packet.txt
```


# dig

query with no eDNS to custom dns

```
dig +retry=0 -p 1053 @127.0.0.1 +noedns google.com
```


Record response_packet.txt 
```
nc -u 8.8.8.8 53 < query_packet.txt > response_packet.txt
```

# Hexdump

```
hexdump -C response_packet.txt
```



## kdig my baby
```
kdig +tls -p 8853 @127.0.0.1 youtube.com +tls-sni="4789191065911556.dns.example.com"
```

