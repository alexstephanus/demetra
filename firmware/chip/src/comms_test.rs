use embassy_executor::task;
use embassy_time::{Duration, Ticker, Timer};
use esp_hal::{
    gpio::Output,
    twai::{EspTwaiFrame, StandardId, Twai},
    uart::UartTx,
    Async,
};

#[task]
pub async fn can_test(mut twai: Twai<'static, Async>, mut stb_pin: Output<'static>) {
    stb_pin.set_low();
    log::info!("[CAN] Transceiver enabled (STB low)");

    Timer::after(Duration::from_millis(10)).await;
    log::info!("[CAN] TWAI started at 250kbit/s");

    let mut counter: u32 = 0;
    let mut ticker = Ticker::every(Duration::from_secs(2));

    loop {
        ticker.next().await;

        let data = counter.to_le_bytes();
        let frame = EspTwaiFrame::new(StandardId::new(0x100).unwrap(), &data).unwrap();

        match twai.transmit_async(&frame).await {
            Ok(()) => {
                log::info!("[CAN] TX frame #{} id=0x100 data={:02x?}", counter, &data);
            }
            Err(e) => {
                log::error!("[CAN] TX error: {:?}", e);
            }
        }

        counter = counter.wrapping_add(1);
    }
}

#[task]
pub async fn rs485_test(mut uart_tx: UartTx<'static, Async>, mut de_pin: Output<'static>) {
    log::info!("[RS485] UART1 started at 9600 baud");

    let mut counter: u32 = 0;
    let mut ticker = Ticker::every(Duration::from_secs(2));

    loop {
        ticker.next().await;

        let mut buf = [0u8; 32];
        let msg = format_msg(&mut buf, counter);

        de_pin.set_high();
        Timer::after(Duration::from_micros(100)).await;

        let mut written = 0;
        while written < msg.len() {
            match uart_tx.write_async(&msg[written..]).await {
                Ok(n) => written += n,
                Err(e) => {
                    log::error!("[RS485] TX error: {:?}", e);
                    break;
                }
            }
        }
        uart_tx.flush_async().await.ok();

        Timer::after(Duration::from_micros(200)).await;
        de_pin.set_low();

        log::info!(
            "[RS485] TX #{}: {:?}",
            counter,
            core::str::from_utf8(msg).unwrap_or("???")
        );

        counter = counter.wrapping_add(1);
    }
}

fn format_msg(buf: &mut [u8; 32], counter: u32) -> &[u8] {
    let prefix = b"DEMETRA #";
    let suffix = b"\r\n";
    let mut pos = prefix.len();
    buf[..pos].copy_from_slice(prefix);

    let mut num_buf = [0u8; 10];
    let num_str = format_u32(counter, &mut num_buf);
    buf[pos..pos + num_str.len()].copy_from_slice(num_str);
    pos += num_str.len();

    buf[pos..pos + suffix.len()].copy_from_slice(suffix);
    pos += suffix.len();

    &buf[..pos]
}

fn format_u32(mut n: u32, buf: &mut [u8; 10]) -> &[u8] {
    if n == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut pos = buf.len();
    while n > 0 {
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        n /= 10;
    }
    &buf[pos..]
}
