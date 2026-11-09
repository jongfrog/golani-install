#!/bin/sh

echo "Shell test"

if [ -n "$BASH_VERSION" ]; then
    echo "Bash 실행"
else
    echo "Bash가 아님"
fi
echo "=============================="
echo " 설치 스크립트 실행 성공!"
echo "=============================="
echo "Hello World!"
