#!/bin/bash

# create storage path if not exists
mkdir -p $STORAGE_PATH

# create database if not exists
touch $STORAGE_DATABASE

# launch storage microservice
fractal-storage --database $STORAGE_DATABASE local --path $STORAGE_PATH
