<div id="top"></div>

[![Contributors][contributors-shield]][contributors-url]
[![Forks][forks-shield]][forks-url]
[![Stargazers][stars-shield]][stars-url]
[![Issues][issues-shield]][issues-url]
[![Build Status][build-status]][build-status-url]
[![MIT License][license-shield]][license-url]
[![LinkedIn][linkedin-shield]][linkedin-url]


<br />
<div align="center">
  <a href="https://github.com/bernii/lsat-proxy-rs">
    <img src="https://raw.githubusercontent.com/bernii/sataddress-rs/7a09f16a116f1211d1e961bc7e78a1add88f6a4e/assets/inv_banner.png" alt="LSAT-Proxy logo" width="80">
  </a>

<h2 align="center">LSAT Proxy (in rust)</h3>
  <p align="center">
    <a href="https://satspay.to"><strong>Live Version</strong></a> | 
    <a href="https://docs.rs/lsat-proxy/latest/lsat-proxy/index.html"><strong>Documentation</strong></a>
    <br />
    <br />
    <a href="https://crates.io/crates/lsat-proxy">Crates.io</a>
    ·
    <a href="https://github.com/bernii/lsat-proxy-rs/issues">Report a Bug</a>
    ·
    <a href="https://github.com/bernii/lsat-proxy-rs/issues">Feature Request</a>
  </p>
</div>


## About The Project

This is a [rust](https://www.rust-lang.org/) implementation of [Lightning Service Authenticvation Token](https://lightningaddress.com/) (aka LSAT) proxy server.

LSAT is a protocol that has been designed both for authentication and as a payment method for APIs using the Lightning Network. You can think of it as **Stripe** for ⚡ payments. The goal is to standardize the payment process when you want to access a paid resource. Some wallets like [Alby](https://getalby.com/) integrate with LSAT allowing you to set budgets and authorize certain micropayment spend for particular apps you're interested with. 

The LSAT-proxy server allows you to quickly and safely protect your paid APIs and create a monetization/payment layer for them. Simply configure amd deploy the LSAT-proxy in front of your appliation server and enojoy gates access to the resrouces/services you provide!

The project consists of **server** and **cli** tool:
* **Server** is the main proxy you use in order to handle the incoing traffic
* **CLI tool** can be used in order to execute some administration tasks and extract usage info.

## Getting Started

If you want to see how it can be used in practice, check out the app using *the latest deployed version* at [ask4.sats.rs](https://ask4.sats.rs/)

The easiest way to run the server is just using the automatically published docker container at [DockerHub](dockerhub-link).

The configuration is fairly stratightforward and is done via `yaml` files. You can find a sample configuration for a single service in the `example-config.yaml` file.

Once you have your config figured out, just run the container:

```bash
$ docker run -v $(pwd)/my-config.yaml:/opt/lsat-proxy/config.yaml -v $(pwd)/lsat-proxy.db:/opt/lsat-proxy/lsat-proxy.db --name lsat-proxy -it --rm lsat-proxy:latest
```

As an alternative, if you're familiar with the rust toolset, you can use [just](https://github.com/casey/just) which will also automatically load your `config.yaml` file. 
```bash
$ just run
```

## Configuration

LSAT-proxy currently supports the following configuration options in the `yaml` file.

```yaml
server:
  host: "0.0.0.0" # IP to bind to
  port: 3030 # port to listen on

lnd:
  host: "https://rockpi-4b.local:10009" # address of the LND node to connect to
  tls_path: "lnd.crt" # path to LND node TLS cert
  mac_path: "lnd.mac" # path to LND admin macaroon

backends: # list of backends to forward traffic to
  - name: "gpt" # name, used in the logs and messages
    path: "/gpt" # path to match when handling the request
    upstream: "https://api.openai.com/v1/completions" # upstream address to forward the traffic to
    proto: "https" # protcol to use
    headers: # list of headers to inject when constructing the request to the upstream
      - "Content-Type: application/json"
      - "Authorization: Bearer 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"
    body: "{\"model\": \"text-davinci-003\", \"prompt\": \"Say this is a test\", \"temperature\": 0, \"max_tokens\": 57}"
    capabilties: ""
    constraints:
      timeout: 600
    price_msat: 200 # mili-sats per api call
    budget_multiple: 5 # number of api calls user can make after making the payment, useful for standard bolt11 payments
    price_passthrough: false # not supported yet
    response_fields: "choices.0.text" # fields to parse and pass back to the client making request to LSAT-proxy
```

## Roadmap
- [~] lsat not found custom error 
- [x] error handling improvements
- [x] code cleanup
- [x] use streaming for getting LND invoice states instead of polling calls
- [x] secret store in some db/embedded? sqllite?
- [x] add to macaroon: path, price?

- [ ] proper errror handling (and x-reason for payment failure, example: QuotaFailure (as per https://lsat.tech/protocol-specification) when out of quota etc)
- [ ] config parsing (mutually exclusive configs)
- [ ] x-reason to when payment required (quota exhaused, no inv associated, wrong path etc)
- [ ] decide on payload in caveats
- [ ] improve tests
- [ ] price_passthrough
- [ ] capabilties
- [ ] grpc handling
- [ ] LSAT payment badge (for paid APIs)
- [ ] add REST API endpoints for data manipulation
- [ ] better error generation & handling


See the [open issues](https://github.com/bernii/lsat-proxy-rs/issues) for a full list of proposed features (and known issues).


## License

Distributed under the MIT License. See `LICENSE` for more information.


## Contact

Bernard Kobos - [@bkobos](https://twitter.com/bkobos) - bkobos+nospam!@gmail.com

Project Link: [https://github.com/bernii/lsat-proxy-rs](https://github.com/bernii/lsat-proxy-rs)

## Acknowledgments

* [LSAT protocol](https://lsat.tech/) documentation that describes how it works, provides specification and the motivation behind it
* [Aperture](https://github.com/lightninglabs/aperture) the original go-based implementation of LSAT by Lightning Labs
* [BTC lightning logo](https://github.com/shocknet/bitcoin-lightning-logo) for creating an open source vector btc logo


<!-- MARKDOWN LINKS & IMAGES -->
<!-- https://www.markdownguide.org/basic-syntax/#reference-style-links -->
[contributors-shield]: https://img.shields.io/github/contributors/bernii/lsat-proxy-rs.svg?style=for-the-badge
[contributors-url]: https://github.com/bernii/lsat-proxy-rs/graphs/contributors
[forks-shield]: https://img.shields.io/github/forks/bernii/lsat-proxy.svg?style=for-the-badge
[forks-url]: https://github.com/bernii/lsat-proxy/network/members
[stars-shield]: https://img.shields.io/github/stars/bernii/lsat-proxy-rs.svg?style=for-the-badge
[stars-url]: https://github.com/bernii/lsat-proxy-rs/stargazers
[issues-shield]: https://img.shields.io/github/issues/bernii/lsat-proxy-rs.svg?style=for-the-badge
[issues-url]: https://github.com/bernii/lsat-proxy-rs/issues
[license-shield]: https://img.shields.io/github/license/bernii/lsat-proxy-rs.svg?style=for-the-badge
[license-url]: https://github.com/bernii/lsat-proxy-rs/blob/main/LICENSE
[linkedin-shield]: https://img.shields.io/badge/-LinkedIn-black.svg?style=for-the-badge&logo=linkedin&colorB=555
[linkedin-url]: https://linkedin.com/in/bernii
[product-screenshot]: images/screenshot.png
[build-status]: https://img.shields.io/endpoint.svg?url=https%3A%2F%2Factions-badge.atrox.dev%2Fbernii%2Flsat-proxy-rs%2Fbadge%3Fref%3Dmain&style=for-the-badge
[build-status-url]: https://actions-badge.atrox.dev/bernii/lsat-proxy-rs/goto?ref=main