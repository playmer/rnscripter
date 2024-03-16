// SPB can only encode/decode RGB streams due to an extremely limited header of only width/height,
// as it's the only unique format we have use of here it would be dishonest to include alpha.
pub struct Image {
    pub pixel_buffer : Vec<[u8; 3]>,
    pub width : u16,
    pub height : u16
}

fn min_bits(value : u8) -> u8 {
    if value == 0 {
        return 0
    }

    let mut value = value;
    let mut bits = 1;
    value >>= 1;
    
    while value != 0 {
        value >>= 1;
        bits += 1;
    }

    bits
}

#[derive(Clone, Copy)]
struct SpbDifference {
    should_add : bool,
    difference : u8
}

struct SpbDifferences {
    // 4 byte clusters of differences, the bool determining if it' an add/subtract, and the 
    // u8 being the difference from the last written byte.
    differences : [SpbDifference; 4],
    bits_to_read : u8
}



// Coming up with good names for these cases is difficult, refer to encode_spb for in-detail usage
// of these cases.
enum SpbHeader {
    Stamp4,                         // 0
    ReadBits(SpbDifferences),       // 1 -> 5, this control code ends up being (bits_to_read - 1), as the decoder
                                    // will do (control + 2) to determine how many bits it'll read:
                                    //   - One bit to get back a bit we need to read to construct the current pixel
                                    //   - One bit to determine add/subtract
    Read4,                          // 6
    ReadBitPlusOne(SpbDifferences), // 7
}


fn bit_distances(last_byte : u8, channel : usize, bytes : &[[u8; 3]; 4]) -> SpbHeader {
    let mut last_byte = last_byte;
    let mut differences : [SpbDifference; 4] = [SpbDifference{should_add : false, difference: 0}; 4];
    let mut bits_to_read = u8::MIN;

    for (i, spb_difference) in differences.iter_mut().enumerate() {
        let channel_byte = bytes[i][channel];

        let should_add = last_byte < channel_byte;
        let initial_difference = last_byte.abs_diff(channel_byte);
        let initial_difference = if should_add {
            last_byte += initial_difference;
            initial_difference - 1 // the decoder adds 1 for us.
        } else {
            last_byte -= initial_difference;
            initial_difference
        };

        *spb_difference = SpbDifference{should_add, difference: initial_difference};

        // Compute the max number of bits we'd need to represent these differences,
        // and thus how many the decoder will need to read. Remember that it'll be the
        // same number of bits read for each byte.
        bits_to_read = bits_to_read.max(min_bits(initial_difference));
    }

    match bits_to_read {
        0 => return SpbHeader::Stamp4,
        1 => return SpbHeader::ReadBitPlusOne(SpbDifferences { differences, bits_to_read }),
        2..=6 => return SpbHeader::ReadBits(SpbDifferences { differences, bits_to_read }),
        _ => return SpbHeader::Read4
    }
}

pub fn encode_spb(image : Image) -> Vec<u8>{
    let mut output_buffer : Vec<u8> = Vec::new();
    use bitbuffer::{BitWriteStream, BigEndian};
    let mut bitstream = BitWriteStream::new(&mut output_buffer, BigEndian);
    let mut image = image;

    // We need to reverse some of the rows as we go left and right across them when writing out data.
    // It's likely faster to do the reversal as we're iterating, (although maybe not due to cache, who knows)
    // but I think it would overcomplicate the code so for now we do a bit of preprocessing here).
    for i in 0..(image.height as usize) {
        if (i & 1) == 0 {
            continue;
        }

        let start = (i * (image.width as usize)) as usize;
        let end = ((i + 1) * (image.width as usize)) as usize;

        image.pixel_buffer[start..end].reverse();
    }

    // Similarly we need some bytes at the end to protect from overflowing, we do the same on reading.
    image.pixel_buffer.push([0; 3]);
    image.pixel_buffer.push([0; 3]);
    image.pixel_buffer.push([0; 3]);
    image.pixel_buffer.push([0; 3]);
    
    // I suspect rows or total pixels need to be divisible by 4
    bitstream.write_int::<u16>(image.width as u16, 16).unwrap();
    bitstream.write_int::<u16>(image.height as u16, 16).unwrap();
    
    let total_pixels = (image.width as usize) * (image.height as usize);

    // Write each channel of image data, in BGR order.
    for channel in 0..3 {
        let mut last_data_byte : u8 = image.pixel_buffer[0][channel];
        bitstream.write_int::<u8>(last_data_byte, 8).unwrap();

        let mut i : usize = 1;
        while i < total_pixels {
            // Encoding occurs in 4 byte segments, with roughly 4 interesting cases, with 2 being
            // only a slight variation on each other:
            match bit_distances(last_data_byte, channel, &image.pixel_buffer[i..i+4].try_into().unwrap()) {
                // Case 1:
                // Next four pixels in this channel are the same as the last byte written. Stamp a control
                // code (0) to signal to a decoder they can stamp 4 more bytes of the channel as-is.
                SpbHeader::Stamp4 => {
                    bitstream.write_int::<u8>(0, 3).unwrap();
                }
                // Case 2:
                // The bytes are sufficiently different such that we can't get to them within a 6 bit
                // add or subtract, so we'll simply stamp a control code (6) to signal we'll be stamping
                // the next 4 pixels of the channel as-is and to read them as-such.
                SpbHeader::Read4 => {
                    bitstream.write_int::<u8>(6, 3).unwrap();
                    bitstream.write_int::<u8>(image.pixel_buffer[i][channel], 8).unwrap();
                    bitstream.write_int::<u8>(image.pixel_buffer[i + 1][channel], 8).unwrap();
                    bitstream.write_int::<u8>(image.pixel_buffer[i + 2][channel], 8).unwrap();
                    bitstream.write_int::<u8>(image.pixel_buffer[i + 3][channel], 8).unwrap();
                    last_data_byte = image.pixel_buffer[i + 3][channel];
                }
                // Case 3:
                // The bytes are so close to the last written that they only deviate by 1
                SpbHeader::ReadBitPlusOne(differences) => {
                    bitstream.write_int::<u8>(7, 3).unwrap();
                    bitstream.write_bool(differences.bits_to_read == 1).unwrap();
                    for spb_difference in differences.differences {
                        if differences.bits_to_read == 1 {
                            bitstream.write_int::<u8>(spb_difference.difference, 1).unwrap();
                        }
                        bitstream.write_bool(spb_difference.should_add).unwrap();
                        
                        if spb_difference.should_add {
                            last_data_byte += spb_difference.difference + 1;
                        } else {
                            last_data_byte -= spb_difference.difference;
                        }
                    }
                }
                // Case 4:
                // The Bytes are with a [2,6] bit range away from the last byte written.
                SpbHeader::ReadBits(differences) => {
                    bitstream.write_int::<u8>(differences.bits_to_read - 1, 3).unwrap();
                    for spb_difference in differences.differences {
                        bitstream.write_int::<u8>(spb_difference.difference, differences.bits_to_read as usize).unwrap();
                        bitstream.write_bool(spb_difference.should_add).unwrap();
                        
                        if spb_difference.should_add {
                            last_data_byte += spb_difference.difference + 1;
                        } else {
                            last_data_byte -= spb_difference.difference;
                        }
                    }
                }
            }

            i += 4;
        }
    }

    output_buffer
}


pub fn decode_spb(buffer : Vec<u8>) -> Vec<u8> {
    use bitbuffer::{BitReadBuffer, BitReadStream, BigEndian};
    let buffer = BitReadBuffer::new(&buffer, BigEndian);
    let mut bitstream = BitReadStream::new(buffer);

    let width = bitstream.read_int::<u16>(16).unwrap() as usize;
    let height = bitstream.read_int::<u16>(16).unwrap() as usize;
    
    let mut pixel_buffer : Vec<u8> = vec![0; (width * height + 4) * 3];

    // Read each channel of image data, in BGR order.
    for channel in (0..3).rev() {
        let start = (width * height + 4) * channel;
        let end = (width * height + 4) * (channel + 1);
        let channel_buffer = &mut pixel_buffer[start..end];
        channel_buffer[0] = bitstream.read_int::<u8>(8).unwrap();
        let mut i : usize = 1;

        while i < (width * height) {
            let mut data_byte = channel_buffer[i - 1];

            // Read a 3 bit header from the stream, 3 bits means range is [0,7]
            // This header helps determine how we stamp the next 4 bytes.
            let header = bitstream.read_int::<u8>(3).unwrap();

            let bits_to_read : u8 = match header {
                // Stamp 4 identical bytes
                0 => {
                    channel_buffer[i]     = data_byte;
                    channel_buffer[i + 1] = data_byte;
                    channel_buffer[i + 2] = data_byte;
                    channel_buffer[i + 3] = data_byte;
                    i += 4;
                    continue;
                }
                6 => {
                    channel_buffer[i]     = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 1] = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 2] = bitstream.read_int::<u8>(8).unwrap();
                    channel_buffer[i + 3] = bitstream.read_int::<u8>(8).unwrap();
                    i += 4;
                    continue;
                }
                // bits_to_read is within  [3,7]
                1..=5 => {
                    header + 2
                }
                // bits_to_read is within [1,2], depending on a 1 bit read.
                // escape sequence in case of adding one or subtracting zero.
                7 => {
                    bitstream.read_int::<u8>(1).unwrap() + 1
                }
                _ => {
                    panic!("Impossible value for n (a 3 bit integer) when decoding SPB: {header}");
                }
            };
            
            // bits_to_read is within [1,7]
            for _ in 0..4 {
                let modify_byte = bitstream.read_int::<u8>(bits_to_read as usize).unwrap();
                
                // The last bit read is used to determine how we'll be modifying the data byte, after
                // determining that we throw away that bit.
                let add = (modify_byte & 1) > 0;
                let modify_byte = modify_byte >> 1;
                
                if add {
                    data_byte += modify_byte + 1;
                } else {
                    data_byte -= modify_byte;
                }

                channel_buffer[i] = data_byte;
                i += 1;
            }
        }
    }

    let r_buffer = &pixel_buffer[0..(width * height + 4)];
    let g_buffer = &pixel_buffer[(width * height + 4)..(width * height + 4) * 2];
    let b_buffer = &pixel_buffer[(width * height + 4) * 2..(width * height + 4) * 3];

    // We've read all the channels, we can comfortably blit out a BMP now.
    let mut bmp_file = bmp_rust::bmp::BMP::new(height as i32, width as u32, None);
    for y in 0..height {
        let row_skip = y * width;
        for x in 0..width {
            // If we're on an odd row, we read backwards
            let i = if (y & 1) == 1 {
                ((width - 1) - x ) + row_skip
            } else {
                x + row_skip
            };

            let r = r_buffer[i];
            let g = g_buffer[i];
            let b = b_buffer[i];
            bmp_file.change_color_of_pixel(x as u16, y as u16, [r,g,b,255]).expect("Failed to change color of pixel");
        }
    }

    bmp_file.contents
}