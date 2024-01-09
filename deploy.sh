#!/bin/sh
set -e
PS3='Please enter your choice: '
options=("build & deploy all" "build all" "deploy all"  "build & deploy rs-algo-bot-backtest" "Quit")
select opt in "${options[@]}"
do
    case $opt in
        "build & deploy all")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-ws-server-backtest:latest rs_algo_ws_server ; docker push cluster.loc:5000/rs-algo-ws-server-backtest:latest ; docker build -t cluster.loc:5000/rs-algo-bot-backtest:latest rs_algo_bot ; docker push cluster.loc:5000/rs-algo-bot-backtest:latest ; ansible-playbook playbook.yml
            break
            ;;
        "build all")
            echo "Deploying: $opt";
            docker build -t cluster.loc:5000/rs-algo-ws-server-backtest:latest rs_algo_ws_server ; docker push cluster.loc:5000/rs-algo-ws-server-backtest:latest ; docker build -t cluster.loc:5000/rs-algo-bot-backtest:latest rs_algo_bot ; docker push cluster.loc:5000/rs-algo-bot-backtest:latest
            break
            ;;
        "deploy all")
            echo "Deploying: $opt";
            ansible-playbook playbook.yml
            break
            ;;
        "build & deploy rs-algo-bot-backtest")
            echo "Deploying: $opt";
           docker build -t cluster.loc:5000/rs-algo-bot-backtest:latest rs_algo_bot ; docker push cluster.loc:5000/rs-algo-bot-backtest:latest ; ansible-playbook playbook.yml
            break
            ;;
        "Quit")
            break
            ;;
        *) echo "invalid option $REPLY";;
    esac
done
