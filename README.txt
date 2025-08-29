Very simple server for obtaining IP address.

This program runs a TCP server which responds to every HTTP request
with the IP address of the sender. It doesn't even check for valid HTTP
and ignores all headers.

OPTIONS
    --help - Display this help text and exit
    --addr <ADDR> - Set address to bind server on (default: 0.0.0.0)
    --port <PORT> - Set port to host server on (default: 80)
    --max-connections <NUMBER> - Set maximum number of simultaneous connections (default: 32)
    --timeout <NUMBER> - Set timeout for requests, in seconds (default: 15)
    --version - Display version number and exit
