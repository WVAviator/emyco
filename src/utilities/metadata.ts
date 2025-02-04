export interface GameboyRomMetadata {
  title: string;
  formattedTitle: string;
  cgb: CGBCompatibility;
  licensee: string;
  sgb: boolean;
  cartridgeType: string;
  romSize: string;
  ramSize: string;
}

export type CGBCompatibility = 'dmgonly' | 'cgbonly' | 'both';

export const extractRomMetadata = (romData: Uint8Array): GameboyRomMetadata => {
  const title = extractTitle(romData);
  const formattedTitle = formatTitle(title);
  const cgb = extractCgb(romData);
  const licensee = extractLicensee(romData);
  const sgb = extractSgb(romData);
  const cartridgeType = extractCartridgeType(romData);
  const romSize = extractRomSize(romData);
  const ramSize = extractRamSize(romData);

  return {
    title,
    formattedTitle,
    cgb,
    licensee,
    sgb,
    cartridgeType,
    romSize,
    ramSize,
  };
};

const extractTitle = (romData: Uint8Array): string => {
  const decoder: TextDecoder = new TextDecoder('ascii');

  const slice = romData.slice(0x134, 0x144);
  let titleEnd = 0;
  for (let i = 0; i < slice.length; i++) {
    titleEnd = i;
    if (slice[i] === 0) {
      break;
    }
  }
  let titleSlice = slice.slice(0, titleEnd);
  const title: string = decoder.decode(titleSlice).trim();

  return title;
};

const formatTitle = (title: string) => {
  const formattedTitle = title
    .split(' ')
    .map((word) => {
      if (word.length > 1) {
        return `${word.slice(0, 1)}${word.slice(1).toLocaleLowerCase()}`;
      }
      return word;
    })
    .join(' ');

  if (formattedTitle in titleCorrections) {
    return titleCorrections[formattedTitle as keyof typeof titleCorrections];
  }

  return formattedTitle;
};

const extractCgb = (romData: Uint8Array): CGBCompatibility => {
  if (romData[0x143] === 0x80) {
    return 'both';
  } else if (romData[0x143] === 0xc0) {
    return 'cgbonly';
  } else {
    return 'dmgonly';
  }
};

const extractLicensee = (romData: Uint8Array): string => {
  const oldLicenseeByte = romData[0x14b];

  if (oldLicenseeByte === 0x33) {
    const decoder: TextDecoder = new TextDecoder('ascii');
    const licenseeBytes = romData.slice(0x144, 0x146);
    const licenseeCode = decoder.decode(licenseeBytes);
    if (licenseeCode in newLicenseeTable) {
      return newLicenseeTable[licenseeCode as keyof typeof newLicenseeTable];
    }

    return `Unknown - 0x33 ${licenseeCode}`;
  } else if (oldLicenseeByte in oldLicenseeTable) {
    return oldLicenseeTable[oldLicenseeByte as keyof typeof oldLicenseeTable];
  }

  return `Unknown - ${oldLicenseeByte.toString(16)}`;
};

const extractSgb = (romData: Uint8Array): boolean => {
  if (romData[0x146] === 0x03) {
    return true;
  }

  return false;
};

const extractCartridgeType = (romData: Uint8Array): string => {
  const cartByte = romData[0x147];
  if (cartByte in cartridgeTypes) {
    return cartridgeTypes[cartByte as keyof typeof cartridgeTypes];
  }

  return `Unknown - ${cartByte.toString(16)}`;
};

const extractRomSize = (romData: Uint8Array): string => {
  let romSizeByte = romData[0x148];
  if (romSizeByte in romSizeTable) {
    return romSizeTable[romSizeByte as keyof typeof romSizeTable];
  }
  return 'Unknown';
};

const extractRamSize = (romData: Uint8Array): string => {
  let ramSizeByte = romData[0x149];
  if (ramSizeByte in ramSizeTable) {
    return ramSizeTable[ramSizeByte as keyof typeof ramSizeTable];
  }
  return 'Unknown';
};

const newLicenseeTable = {
  '00': 'None',
  '01': 'Nintendo Research and Development 1',
  '08': 'Capcom',
  '13': 'Electronic Arts',
  '18': 'Hudson Soft',
  '19': 'B - AI',
  '20': 'KSS',
  '22': 'Planning Office WADA',
  '24': 'PCM Complete',
  '25': 'San-X',
  '28': 'Kemco',
  '29': 'SETA Corporation',
  '30': 'Viacom',
  '31': 'Nintendo',
  '32': 'Bandai',
  '33': 'Ocean Software/Acclaim Entertainment',
  '34': 'Konami',
  '35': 'HectorSoft',
  '37': 'Taito',
  '38': 'Hudson Soft',
  '39': 'Banpresto',
  '41': 'Ubi Soft',
  '42': 'Atlus',
  '44': 'Malibu Interative',
  '46': 'Angel',
  '47': 'Bullet-Proof Software',
  '49': 'Irem',
  '50': 'Absolute',
  '51': 'Acclaim Entertainment',
  '52': 'Activision',
  '53': 'Sammy USA Corporation',
  '54': 'Konami',
  '55': 'High Tech Expressions',
  '56': 'LJN',
  '57': 'Matchbox',
  '58': 'Mattel',
  '59': 'Milton Bradley Company',
  '60': 'Titus Interactive',
  '61': 'Virgin Games Ltd.',
  '64': 'Lucasfilm Games',
  '67': 'Ocean Software',
  '69': 'EA (Electronic Arts)',
  '70': 'Infogrames',
  '71': 'Interplay Entertainment',
  '72': 'Broderbund',
  '73': 'Sculptured Software',
  '75': 'The Sales Curve Limited',
  '78': 'THQ',
  '79': 'Accolade',
  '80': 'Misawa Entertainment',
  '83': 'Iozc',
  '86': 'Tokuma Shoten',
  '87': 'Tsukuda Original',
  '90': 'Chunsoft Co',
  '92': 'Video System',
  '93': 'Ocean Software/Acclaim Entertainment',
  '95': 'Varie',
  '96': 'Yonezawa/s’pal',
  '97': 'Kaneko',
  '99': 'Pack-In-Video',
  '9H': 'Bottom Up',
  A4: 'Konami',
  BL: 'MTO',
  DK: 'Kodansha',
};

const cartridgeTypes = {
  0x00: 'ROM ONLY',
  0x01: 'MBC1',
  0x02: 'MBC1+RAM',
  0x03: 'MBC1+RAM+BATTERY',
  0x05: 'MBC2',
  0x06: 'MBC2+BATTERY',
  0x08: 'ROM+RAM 9',
  0x09: 'ROM+RAM+BATTERY 9',
  0x0b: 'MMM01',
  0x0c: 'MMM01+RAM',
  0x0d: 'MMM01+RAM+BATTERY',
  0x0f: 'MBC3+TIMER+BATTERY',
  0x10: 'MBC3+TIMER+RAM+BATTERY 10',
  0x11: 'MBC3',
  0x12: 'MBC3+RAM 10',
  0x13: 'MBC3+RAM+BATTERY 10',
  0x19: 'MBC5',
  0x1a: 'MBC5+RAM',
  0x1b: 'MBC5+RAM+BATTERY',
  0x1c: 'MBC5+RUMBLE',
  0x1d: 'MBC5+RUMBLE+RAM',
  0x1e: 'MBC5+RUMBLE+RAM+BATTERY',
  0x20: 'MBC6',
  0x22: 'MBC7+SENSOR+RUMBLE+RAM+BATTERY',
  0xfc: 'POCKET CAMERA',
  0xfd: 'BANDAI TAMA5',
  0xfe: 'HuC3',
  0xff: 'HuC1+RAM+BATTERY',
};

const oldLicenseeTable = {
  0x00: 'None',
  0x01: 'Nintendo',
  0x08: 'Capcom',
  0x09: 'HOT-B',
  0x0a: 'Jaleco',
  0x0b: 'Coconuts Japan',
  0x0c: 'Elite Systems',
  0x13: 'EA (Electronic Arts)',
  0x18: 'Hudson Soft',
  0x19: 'ITC Entertainment',
  0x1a: 'Yanoman',
  0x1d: 'Japan Clary',
  0x1f: 'Virgin Games Ltd.3',
  0x24: 'PCM Complete',
  0x25: 'San-X',
  0x28: 'Kemco',
  0x29: 'SETA Corporation',
  0x30: 'Infogrames5',
  0x31: 'Nintendo',
  0x32: 'Bandai',
  0x34: 'Konami',
  0x35: 'HectorSoft',
  0x38: 'Capcom',
  0x39: 'Banpresto',
  0x3c: 'Entertainment Interactive (stub)',
  0x3e: 'Gremlin',
  0x41: 'Ubi Soft1',
  0x42: 'Atlus',
  0x44: 'Malibu Interactive',
  0x46: 'Angel',
  0x47: 'Spectrum HoloByte',
  0x49: 'Irem',
  0x4a: 'Virgin Games Ltd.3',
  0x4d: 'Malibu Interactive',
  0x4f: 'U.S. Gold',
  0x50: 'Absolute',
  0x51: 'Acclaim Entertainment',
  0x52: 'Activision',
  0x53: 'Sammy USA Corporation',
  0x54: 'GameTek',
  0x55: 'Park Place13',
  0x56: 'LJN',
  0x57: 'Matchbox',
  0x59: 'Milton Bradley Company',
  0x5a: 'Mindscape',
  0x5b: 'Romstar',
  0x5c: 'Naxat Soft14',
  0x5d: 'Tradewest',
  0x60: 'Titus Interactive',
  0x61: 'Virgin Games Ltd.3',
  0x67: 'Ocean Software',
  0x69: 'EA (Electronic Arts)',
  0x6e: 'Elite Systems',
  0x6f: 'Electro Brain',
  0x70: 'Infogrames5',
  0x71: 'Interplay Entertainment',
  0x72: 'Broderbund',
  0x73: 'Sculptured Software6',
  0x75: 'The Sales Curve Limited7',
  0x78: 'THQ',
  0x79: 'Accolade15',
  0x7a: 'Triffix Entertainment',
  0x7c: 'MicroProse',
  0x7f: 'Kemco',
  0x80: 'Misawa Entertainment',
  0x83: 'LOZC G.',
  0x86: 'Tokuma Shoten',
  0x8b: 'Bullet-Proof Software2',
  0x8c: 'Vic Tokai Corp.16',
  0x8e: 'Ape Inc.17',
  0x8f: "I'Max18",
  0x91: 'Chunsoft Co.8',
  0x92: 'Video System',
  0x93: 'Tsubaraya Productions',
  0x95: 'Varie',
  0x96: "Yonezawa19/S'Pal",
  0x97: 'Kemco',
  0x99: 'Arc',
  0x9a: 'Nihon Bussan',
  0x9b: 'Tecmo',
  0x9c: 'Imagineer',
  0x9d: 'Banpresto',
  0x9f: 'Nova',
  0xa1: 'Hori Electric',
  0xa2: 'Bandai',
  0xa4: 'Konami',
  0xa6: 'Kawada',
  0xa7: 'Takara',
  0xa9: 'Technos Japan',
  0xaa: 'Broderbund',
  0xac: 'Toei Animation',
  0xad: 'Toho',
  0xaf: 'Namco',
  0xb0: 'Acclaim Entertainment',
  0xb1: 'ASCII Corporation or Nexsoft',
  0xb2: 'Bandai',
  0xb4: 'Square Enix',
  0xb6: 'HAL Laboratory',
  0xb7: 'SNK',
  0xb9: 'Pony Canyon',
  0xba: 'Culture Brain',
  0xbb: 'Sunsoft',
  0xbd: 'Sony Imagesoft',
  0xbf: 'Sammy Corporation',
  0xc0: 'Taito',
  0xc2: 'Kemco',
  0xc3: 'Square',
  0xc4: 'Tokuma Shoten',
  0xc5: 'Data East',
  0xc6: 'Tonkin House',
  0xc8: 'Koei',
  0xc9: 'UFL',
  0xca: 'Ultra Games',
  0xcb: 'VAP, Inc.',
  0xcc: 'Use Corporation',
  0xcd: 'Meldac',
  0xce: 'Pony Canyon',
  0xcf: 'Angel',
  0xd0: 'Taito',
  0xd1: 'SOFEL (Software Engineering Lab)',
  0xd2: 'Quest',
  0xd3: 'Sigma Enterprises',
  0xd4: 'ASK Kodansha Co.',
  0xd6: 'Naxat Soft14',
  0xd7: 'Copya System',
  0xd9: 'Banpresto',
  0xda: 'Tomy',
  0xdb: 'LJN',
  0xdd: 'Nippon Computer Systems',
  0xde: 'Human Ent.',
  0xdf: 'Altron',
  0xe0: 'Jaleco',
  0xe1: 'Towa Chiki',
  0xe2: 'Yutaka',
  0xe3: 'Varie',
  0xe5: 'Epoch',
  0xe7: 'Athena',
  0xe8: 'Asmik Ace Entertainment',
  0xe9: 'Natsume',
  0xea: 'King Records',
  0xeb: 'Atlus',
  0xec: 'Epic/Sony Records',
  0xee: 'IGS',
  0xf0: 'A Wave',
  0xf3: 'Extreme Entertainment',
  0xff: 'LJN',
};

const romSizeTable = {
  0x00: '32 KiB',
  0x01: '64 KiB',
  0x02: '128 KiB',
  0x03: '256 KiB',
  0x04: '512 KiB',
  0x05: '1 MiB',
  0x06: '2 MiB',
  0x07: '4 MiB',
  0x08: '8 MiB',
};

const ramSizeTable = {
  0x00: 'No RAM',
  0x02: '8 KiB',
  0x03: '32 KiB',
  0x04: '128 KiB',
  0x05: '64 KiB',
};

const titleCorrections = {
  'Dr.mario': 'Dr. Mario',
};
