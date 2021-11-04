#!/bin/bash

# create storage path if not exists
mkdir -p $STORAGE_PATH

# create database if not exists
touch $STORAGE_DATABASE

# launch storage microservice
storage --database $STORAGE_DATABASE --storage $STORAGE_PATH