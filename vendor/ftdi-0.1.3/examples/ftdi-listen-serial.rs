use structopt::StructOpt;

#[derive(StructOpt)]
struct Args {
    #[structopt(long)]
    index: u32,
    #[structopt(long)]
    baud_rate: u32,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::from_args();
    let mut channel = ftdi::find_by_vid_pid(0x0403, 0x6015)
        .nth(args.index)
        .open()?;
    channel.configure(ftdi::Bits::Eight, ftdi::StopBits::One, ftdi::Parity::Odd)?;
    channel.set_baud_rate(args.baud_rate)?;
    loop {
        let mut buffer = [0; 4096];
        let len = channel.read_packet(&mut buffer)?;
        println!("data: {} bytes, start {:0x?}", len, &buffer[..30]);
    }
}
