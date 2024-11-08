Unknown applicability:
- bits, stop_bits, parity, break_type
- read/write
- chunksize
- bitmode / bitmask
- en/disable bitbang (sets bitmode internally)
- read pins
- modem status(???)
- flow control
- set: DTR / RTS
- event char
- error char
- (eeprom)
  - strings
  - CBUS
  - (everything else)
  - user data
  - chip ID




BITMODE_RESET: switch off bitbang mode, back to regular serial/FIFO
- FIFO needs eeprom
BITMODE_BITBANG: classical asynchronous bitbang mode, introduced with B-type chips
- no eeprom
BITMODE_MPSSE: MPSSE mode, available on 2232x chips
BITMODE_SYNCBB: synchronous bitbang mode, available on 2232x and R-type chips
- no eeprom
BITMODE_MCU: MCU Host Bus Emulation mode, available on 2232x chips
BITMODE_OPTO: Fast Opto-Isolated Serial Interface Mode, available on 2232x chips
- channel B only(?)
BITMODE_CBUS: Bitbang on CBUS pins of R-type chips, configure in EEPROM before
BITMODE_SYNCFF: Single Channel Synchronous FIFO mode, available on 2232H chips
- single channel only, always on A
- needs eeprom
BITMODE_FT1284: FT1284 mode, available on 232H chips
- according to app note, it's FT1248(sic!)

Legend:
AB / A-only / B-only: channels that can be used
DC: dual-channel (requires both)
EE: requires EEPROM
NB: does not use baud rate
CS: uses a command stream, not data stream

As in 2232H docs:
- RS232 (BITMODE_RESET) AB
- FT245 sync FIFO (BITMODE_SYNCFF) A-only DC EE NB [1] [4]
- FT245 async FIFO (BITMODE_RESET?) AB EE NB
- Async bitbang (BITMODE_BITBANG) AB
- Sync bitbang (BITMODE_SYNCBB) AB
- MPSSE (BITMODE_MPSSE) AB CS NB
- Fast Serial (BITMODE_OPTO) AB !DC EE NB [2] [3]
- CPU-style FIFO (???) AB EE NB???
- Host Bus Emulation Interface (BITMODE_MCU) AB DC NB CS
In 232H also:
- FT1248 Dynamic Parallel/SerialInterface AB NB EE

[1]: both channels must be in async FIFO before
[2]: Always uses channel B pins, but can be enabled on any channel (or both!)
[3]: According to FT2232H manual, BITMODE_OPTO is actually its "hold" config, and regular is BITMODE_RESET(!)
[4]: It may be desirable to change channel B to inputs with MPSSE before entering this mode

So, for FT2232H the following branches are EEPROM-chosen
- RS232
- FIFO -> SYNCFF
- OPTO
- cpuFIFO

The following are enter at will:
- BB
- SYNCBB
- MPSSE
- MCU

Unknown, probably needs EEPROM
- FT1248

Baud rate affects RS232, BB, SYNCBB
Command stream instead of data: MPSSE, MCU
## Does async BB read???
## what is FT1284?




0x0 = Reset0x1 = Asynchronous Bit Bang
0x2 = MPSSE (FT2232, FT2232H, FT4232H and FT232Hdevices only)
0x4 = Synchronous Bit Bang (FT232R, FT245R,FT2232, FT2232H, FT4232H and FT232Hdevices only)
0x8 = MCU Host Bus Emulation Mode (FT2232, FT2232H, FT4232Hand FT232Hdevices only)
0x10 = FastOpto-Isolated Serial Mode (FT2232, FT2232H, FT4232H and FT232Hdevices only)
0x20 = CBUS Bit Bang Mode (FT232Rand FT232Hdevices only)
0x40 = Single Channel Synchronous 245 FIFO Mode (FT2232H and FT232H devices only)



