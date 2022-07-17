#!/usr/bin/env bash
set -eu



for TARGET in artifacts/*; do
    echo $TARGET
    
    mkdir -p ./packages/$1/$TARGET

    cp $TARGET/$1/$1.*.node ./packages/$1/$TARGET
done