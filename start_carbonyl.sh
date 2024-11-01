#!/bin/bash

pipe=/tmp/my_pipe

echo "waiting for server to start "

read line < $pipe

carbonyl "http://127.0.0.1:3333" 

