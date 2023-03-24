#!/bin/sh
set -e
PS3='Please enter your choice: '
options=("build & deploy all" "build all" "deploy all" "build & deploy rs-algo-ws-server" "build & deploy rs-algo-bot" "Quit")
select opt in "${options[@]}"
do
    case $opt in
        "build & deploy all")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-ws-server:latest rs_algo_ws_server ; docker build -t cluster.loc:5000/rs-algo-bot:latest rs_algo_bot ; docker push cluster.loc:5000/rs-algo-ws-server:latest ; docker push cluster.loc:5000/rs-algo-bot:latest ; ansible-playbook playbook.yml
            break
            ;;
        "build all")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-ws-server:latest rs_algo_ws_server ; docker build -t cluster.loc:5000/rs-algo-bot:latest rs_algo_bot
            break
            ;;
        "deploy all")
            echo "Deploying: $opt";
            ansible-playbook playbook.yml
            break
            ;;
        "build & deploy rs-algo-ws-server")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-ws-server:latest rs_algo_ws_server ; docker push cluster.loc:5000/rs-algo-ws-server:latest ; ansible-playbook playbook.yml
            break
            ;;
        "build & deploy rs-algo-bot")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-bot:latest rs_algo_bot ; docker push cluster.loc:5000/rs-algo-bot:latest ; ansible-playbook playbook.yml
            break
            ;;
        "Quit")
            break
            ;;
        *) echo "invalid option $REPLY";;
    esac
done
