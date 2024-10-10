# Tor Flow Simulator (TorFS)

The Tor Flow Simulator (TorFS) enables the experimentation with user flows
within a simulated Tor network.
Similarly to [torps](https://github.com/torps/torps),
it simulates circuit construction and circuit handling
based on a re-implementation of the high-level logic implemented in the Tor client.
However, TorFS adds another dimension by also simulating users and flows (streams).
It can thus generate synthetic packet traces that can be further analyzed thereafter.
Moreover, TorFS aims to be scalable and usable with current as well as future Tor consensuses,
avoiding performance deficiencies known from torps.

In its current state, TorFS has been implemented primarily
for generating the output needed for our research
but may be expanded to cover more use cases of torps.

This is joint work of Christoph Döpmann, Maximilian Weisenseel and Florian Tschorsch,
and is thoroughly introduced in the following scientific work (to appear):

> **On the Evolution of Onion Routing Networks**, Christoph Döpmann, PhD thesis

## License

This project is licensed under the terms of the GNU General Public License v3.0 (only).
