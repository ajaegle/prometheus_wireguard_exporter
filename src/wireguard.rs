use crate::exporter_error::ExporterError;
use crate::wireguard_config::PeerEntryHashMap;
use log::{debug, trace};
use prometheus_exporter_base::PrometheusCounter;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::net::SocketAddr;

const EMPTY: &str = "(none)";

#[derive(Default, Debug, Clone)]
pub(crate) struct LocalEndpoint {
    pub public_key: String,
    pub private_key: String,
    pub local_port: u16,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteEndpoint {
    pub public_key: String,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u16>,
    pub local_ip: String,
    pub local_subnet: String,
    pub latest_handshake: u64,
    pub sent_bytes: u128,
    pub received_bytes: u128,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Endpoint {
    Local(LocalEndpoint),
    Remote(RemoteEndpoint),
}

fn to_option_string(s: &str) -> Option<String> {
    if s == EMPTY {
        None
    } else {
        Some(s.to_owned())
    }
}

fn to_bool(s: &str) -> bool {
    s != "off"
}

#[derive(Debug, Clone)]
pub(crate) struct WireGuard {
    pub interfaces: HashMap<String, Vec<Endpoint>>,
}

impl TryFrom<&str> for WireGuard {
    type Error = ExporterError;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        debug!("wireguard::try_from({}) called", input);
        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        for line in input.lines() {
            let v: Vec<&str> = line.split('\t').filter(|s| !s.is_empty()).collect();
            debug!("v == {:?}", v);

            let endpoint = if v.len() == 5 {
                // this is the local interface
                Endpoint::Local(LocalEndpoint {
                    public_key: v[1].to_owned(),
                    private_key: v[2].to_owned(),
                    local_port: v[3].parse::<u16>().unwrap(),
                    persistent_keepalive: to_bool(v[4]),
                })
            } else {
                // remote endpoint
                let public_key = v[1].to_owned();

                let (remote_ip, remote_port) = if let Some(ip_and_port) = to_option_string(v[3]) {
                    let addr: SocketAddr = ip_and_port.parse::<SocketAddr>().unwrap();

                    (Some(addr.ip().to_string()), Some(addr.port()))
                } else {
                    (None, None)
                };

                let tok: Vec<&str> = v[4].split('/').collect();
                let (local_ip, local_subnet) = (tok[0].to_owned(), tok[1].to_owned());

                Endpoint::Remote(RemoteEndpoint {
                    public_key,
                    remote_ip,
                    remote_port,
                    local_ip,
                    local_subnet,
                    latest_handshake: v[5].parse::<u64>()?,
                    sent_bytes: v[6].parse::<u128>().unwrap(),
                    received_bytes: v[7].parse::<u128>().unwrap(),
                    persistent_keepalive: to_bool(v[8]),
                })
            };

            trace!("{:?}", endpoint);

            if let Some(endpoints) = wg.interfaces.get_mut(v[0]) {
                endpoints.push(endpoint);
            } else {
                let mut new_vec = Vec::new();
                new_vec.push(endpoint);
                wg.interfaces.insert(v[0].to_owned(), new_vec);
            }
        }

        trace!("{:?}", wg);
        Ok(wg)
    }
}

impl WireGuard {
    pub(crate) fn render_with_names(&self, pehm: Option<&PeerEntryHashMap>) -> String {
        // these are the exported counters
        let pc_sent_bytes_total = PrometheusCounter::new(
            "wireguard_sent_bytes_total",
            "counter",
            "Bytes sent to the peer",
        );
        let pc_received_bytes_total = PrometheusCounter::new(
            "wireguard_received_bytes_total",
            "counter",
            "Bytes received from the peer",
        );
        let pc_latest_handshake = PrometheusCounter::new(
            "wireguard_latest_handshake_seconds",
            "gauge",
            "Seconds from the last handshake",
        );

        // these 3 vectors will hold the intermediate
        // values. We use the vector in order to traverse
        // the interfaces slice only once: since we need to output
        // the values grouped by counter we populate the vectors here
        // and then reorder during the final string creation phase.
        let mut s_sent_bytes_total = Vec::new();
        s_sent_bytes_total.push(pc_sent_bytes_total.render_header());

        let mut s_received_bytes_total = Vec::new();
        s_received_bytes_total.push(pc_received_bytes_total.render_header());

        let mut s_latest_handshake = Vec::new();
        s_latest_handshake.push(pc_latest_handshake.render_header());

        for (interface, endpoints) in self.interfaces.iter() {
            for endpoint in endpoints {
                // only show remote endpoints
                if let Endpoint::Remote(ep) = endpoint {
                    debug!("{:?}", ep);

                    let mut attributes: Vec<(&str, &str)> = Vec::new();
                    attributes.push(("inteface", interface));
                    attributes.push(("public_key", &ep.public_key));
                    attributes.push(("local_ip", &ep.local_ip));
                    attributes.push(("local_subnet", &ep.local_subnet));

                    // let's add the friendly_name attribute if present
                    // and has meaniningful value
                    if let Some(pehm) = pehm {
                        if let Some(ep_friendly_name) = pehm.get(&ep.public_key as &str) {
                            if let Some(ep_friendly_name) = ep_friendly_name.name {
                                attributes.push(("friendly_name", &ep_friendly_name));
                            }
                        }
                    }

                    s_sent_bytes_total
                        .push(pc_sent_bytes_total.render_counter(Some(&attributes), ep.sent_bytes));
                    s_received_bytes_total.push(
                        pc_received_bytes_total
                            .render_counter(Some(&attributes), ep.received_bytes),
                    );
                    s_latest_handshake.push(
                        pc_latest_handshake.render_counter(Some(&attributes), ep.latest_handshake),
                    );
                }
            }
        }

        // now let's join the results and return it to the caller
        let mut s = String::with_capacity(s_latest_handshake.len() * 64 * 3);
        for item in s_sent_bytes_total {
            s.push_str(&item);
        }
        for item in s_received_bytes_total {
            s.push_str(&item);
        }
        for item in s_latest_handshake {
            s.push_str(&item);
        }

        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT : &'static str = "wg0\t000q4qAC0ExW/BuGSmVR1nxH9JAXT6g9Wd3oEGy5lA=\t0000u8LWR682knVm350lnuqlCJzw5SNLW9Nf96P+m8=\t51820\toff
wg0\t2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk=\t(none)\t37.159.76.245:29159\t10.70.0.2/32\t1555771458\t10288508\t139524160\toff
wg0\tqnoxQoQI8KKMupLnSSureORV0wMmH7JryZNsmGVISzU=\t(none)\t(none)\t10.70.0.3/32\t0\t0\t0\toff
wg0\tL2UoJZN7RmEKsMmqaJgKG0m1S2Zs2wd2ptAf+kb3008=\t(none)\t(none)\t10.70.0.4/32\t0\t0\t0\toff
wg0\tMdVOIPKt9K2MPj/sO2NlWQbOnFJ6L/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.0.50/32\t0\t0\t0\toff
wg2\tMdVOIPKt9K2MPj/sO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.5.50/32\t0\t0\t0\toff
pollo\tYdVOIPKt9K2MPsO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.70.50/32\t0\t0\t0\toff
wg0\t928vO9Lf4+Mo84cWu4k1oRyzf0AR7FTGoPKHGoTMSHk=\t(none)\t5.90.62.106:21741\t10.70.0.80/32\t1555344925\t283012\t6604620\toff
";

    #[test]
    fn test_parse() {
        let a = WireGuard::try_from(TEXT).unwrap();
        println!("{:?}", a);
        assert!(a.interfaces.len() == 3);
        assert!(a.interfaces["wg0"].len() == 6);

        let e1 = match &a.interfaces["wg0"][1] {
            Endpoint::Local(_) => panic!(),
            Endpoint::Remote(re) => re,
        };

        assert_eq!(
            e1.public_key,
            "2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk="
        );
    }

    #[test]
    fn test_parse_and_serialize() {
        let a = WireGuard::try_from(TEXT).unwrap();
        let s = a.render_with_names(None);
        println!("{}", s);
    }

    #[test]
    fn test_render_to_prometheus_simple() {
        const REF : &str= "# HELP wireguard_sent_bytes_total Bytes sent to the peer\n# TYPE wireguard_sent_bytes_total counter\nwireguard_sent_bytes_total{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 1000\n# HELP wireguard_received_bytes_total Bytes received from the peer\n# TYPE wireguard_received_bytes_total counter\nwireguard_received_bytes_total{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 5000\n# HELP wireguard_latest_handshake_seconds Seconds from the last handshake\n# TYPE wireguard_latest_handshake_seconds gauge\nwireguard_latest_handshake_seconds{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 500\n";

        let re = Endpoint::Remote(RemoteEndpoint {
            public_key: "test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            local_ip: "local_ip".to_owned(),
            local_subnet: "local_subnet".to_owned(),
            latest_handshake: 500,
            sent_bytes: 1000,
            received_bytes: 5000,
            persistent_keepalive: false,
        });
        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        let mut v = Vec::new();
        v.push(re);
        wg.interfaces.insert("Pippo".to_owned(), v);

        let prometheus = wg.render_with_names(None);

        assert_eq!(prometheus, REF);
    }

    #[test]
    fn test_render_to_prometheus_complex() {
        use crate::wireguard_config::PeerEntry;

        const REF :&'static str = "# HELP wireguard_sent_bytes_total Bytes sent to the peer\n# TYPE wireguard_sent_bytes_total counter\nwireguard_sent_bytes_total{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 1000\nwireguard_sent_bytes_total{inteface=\"Pippo\",public_key=\"second_test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\",friendly_name=\"this is my friendly name\"} 14\n# HELP wireguard_received_bytes_total Bytes received from the peer\n# TYPE wireguard_received_bytes_total counter\nwireguard_received_bytes_total{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 5000\nwireguard_received_bytes_total{inteface=\"Pippo\",public_key=\"second_test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\",friendly_name=\"this is my friendly name\"} 1000000000\n# HELP wireguard_latest_handshake_seconds Seconds from the last handshake\n# TYPE wireguard_latest_handshake_seconds gauge\nwireguard_latest_handshake_seconds{inteface=\"Pippo\",public_key=\"test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\"} 500\nwireguard_latest_handshake_seconds{inteface=\"Pippo\",public_key=\"second_test\",local_ip=\"local_ip\",local_subnet=\"local_subnet\",friendly_name=\"this is my friendly name\"} 50\n";

        let re1 = Endpoint::Remote(RemoteEndpoint {
            public_key: "test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            local_ip: "local_ip".to_owned(),
            local_subnet: "local_subnet".to_owned(),
            latest_handshake: 500,
            sent_bytes: 1000,
            received_bytes: 5000,
            persistent_keepalive: false,
        });
        let re2 = Endpoint::Remote(RemoteEndpoint {
            public_key: "second_test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            local_ip: "local_ip".to_owned(),
            local_subnet: "local_subnet".to_owned(),
            latest_handshake: 50,
            sent_bytes: 14,
            received_bytes: 1_000_000_000,
            persistent_keepalive: false,
        });

        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        let mut v = Vec::new();
        v.push(re1);
        v.push(re2);
        wg.interfaces.insert("Pippo".to_owned(), v);

        let mut pehm = PeerEntryHashMap::new();
        let pe = PeerEntry {
            public_key: "second_test",
            allowed_ips: "ignored",
            name: Some("this is my friendly name"),
        };
        pehm.insert(pe.public_key, pe);

        let prometheus = wg.render_with_names(Some(&pehm));

        assert_eq!(prometheus, REF);
    }

}
