use anyhow::Result;
use colored::Colorize;
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct SubnetInfo {
    pub input_ip: String,
    pub cidr: u8,
    pub network: String,
    pub broadcast: String,
    pub subnet_mask: String,
    pub wildcard_mask: String,
    pub first_host: String,
    pub last_host: String,
    pub total_hosts: u64,
    pub usable_hosts: u64,
    pub ip_class: String,
    pub ip_type: String,
    pub binary_network: String,
    pub binary_mask: String,
    pub binary_broadcast: String,
}

fn ip_to_u32(ip: &str) -> Option<u32> {
    let octets: Vec<u8> = ip.split('.').filter_map(|s| s.parse::<u8>().ok()).collect();
    if octets.len() != 4 {
        return None;
    }
    Some(
        (octets[0] as u32) << 24
            | (octets[1] as u32) << 16
            | (octets[2] as u32) << 8
            | (octets[3] as u32),
    )
}

fn u32_to_ip(n: u32) -> String {
    format!(
        "{}.{}.{}.{}",
        (n >> 24) & 0xFF,
        (n >> 16) & 0xFF,
        (n >> 8) & 0xFF,
        n & 0xFF
    )
}

fn u32_to_binary(n: u32) -> String {
    format!(
        "{:08b}.{:08b}.{:08b}.{:08b}",
        (n >> 24) & 0xFF,
        (n >> 16) & 0xFF,
        (n >> 8) & 0xFF,
        n & 0xFF
    )
}

fn ip_class(first_octet: u8) -> &'static str {
    match first_octet {
        0..=127 => "Class A",
        128..=191 => "Class B",
        192..=223 => "Class C",
        224..=239 => "Class D (Multicast)",
        240..=255 => "Class E (Reserved)",
    }
}

fn ip_type(ip_num: u32) -> &'static str {
    let a = (ip_num >> 24) & 0xFF;
    let b = (ip_num >> 16) & 0xFF;

    if a == 10 {
        return "Private (RFC 1918)";
    }
    if a == 172 && (16..=31).contains(&b) {
        return "Private (RFC 1918)";
    }
    if a == 192 && b == 168 {
        return "Private (RFC 1918)";
    }
    if a == 127 {
        return "Loopback";
    }
    if a == 169 && b == 254 {
        return "Link-Local (APIPA)";
    }
    if a >= 224 && a <= 239 {
        return "Multicast";
    }
    if a >= 240 {
        return "Reserved";
    }
    "Public"
}

pub fn calculate(ip: &str, cidr: u8) -> Result<SubnetInfo> {
    if cidr > 32 {
        anyhow::bail!("CIDR prefix must be 0-32, got {}", cidr);
    }

    let ip_num = ip_to_u32(ip).ok_or_else(|| anyhow::anyhow!("Invalid IP address: {}", ip))?;

    let mask = if cidr == 0 {
        0u32
    } else {
        0xFFFFFFFFu32 << (32 - cidr)
    };
    let wildcard = !mask;

    let network = ip_num & mask;
    let broadcast = network | wildcard;

    let total_hosts = 1u64 << (32 - cidr);
    let (usable_hosts, first_host, last_host) = if cidr == 32 {
        (1u64, network, network)
    } else if cidr == 31 {
        (2u64, network, broadcast)
    } else {
        (total_hosts - 2, network + 1, broadcast - 1)
    };

    let first_octet = (ip_num >> 24) as u8;

    Ok(SubnetInfo {
        input_ip: ip.to_string(),
        cidr,
        network: u32_to_ip(network),
        broadcast: u32_to_ip(broadcast),
        subnet_mask: u32_to_ip(mask),
        wildcard_mask: u32_to_ip(wildcard),
        first_host: u32_to_ip(first_host),
        last_host: u32_to_ip(last_host),
        total_hosts,
        usable_hosts,
        ip_class: ip_class(first_octet).to_string(),
        ip_type: ip_type(ip_num).to_string(),
        binary_network: u32_to_binary(network),
        binary_mask: u32_to_binary(mask),
        binary_broadcast: u32_to_binary(broadcast),
    })
}

/// Parse CIDR notation (e.g., "192.168.1.0/24")
pub fn parse_cidr(input: &str) -> Result<(String, u8)> {
    let parts: Vec<&str> = input.split('/').collect();
    if parts.len() != 2 {
        anyhow::bail!("Invalid CIDR notation: {}. Expected format: X.X.X.X/N", input);
    }
    let ip = parts[0].trim().to_string();
    let cidr: u8 = parts[1]
        .trim()
        .parse()
        .map_err(|_| anyhow::anyhow!("Invalid CIDR prefix: {}", parts[1]))?;
    Ok((ip, cidr))
}

/// CLI: run subnet calculator
pub fn run(input: &str) -> Result<()> {
    let (ip, cidr) = parse_cidr(input)?;
    let info = calculate(&ip, cidr)?;

    println!("{} {}/{}", "Input:".cyan(), info.input_ip, info.cidr);
    println!();
    println!("{}", "Network Information".green());
    println!("  {:<20} {}", "Network Address:", info.network);
    println!("  {:<20} {}", "Broadcast Address:", info.broadcast);
    println!("  {:<20} {} (/{})","Subnet Mask:", info.subnet_mask, info.cidr);
    println!("  {:<20} {}", "Wildcard Mask:", info.wildcard_mask);
    println!();
    println!("{}", "Host Information".green());
    println!("  {:<20} {}", "Total Hosts:", info.total_hosts);
    println!("  {:<20} {}", "Usable Hosts:", info.usable_hosts);
    println!("  {:<20} {}", "First Host:", info.first_host);
    println!("  {:<20} {}", "Last Host:", info.last_host);
    println!();
    println!("{}", "Classification".green());
    println!("  {:<20} {}", "IP Class:", info.ip_class);
    println!("  {:<20} {}", "Type:", info.ip_type);
    println!();
    println!("{}", "Binary Representation".green());
    println!("  {:<20} {}", "Network:", info.binary_network);
    println!("  {:<20} {}", "Mask:", info.binary_mask);
    println!("  {:<20} {}", "Broadcast:", info.binary_broadcast);
    println!();

    Ok(())
}
