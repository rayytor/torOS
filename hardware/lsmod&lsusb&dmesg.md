rayyan@rayyan:~
$ lsmod
Module                  Size  Used by
hid_logitech_hidpp     69632  0
hid_logitech_dj        40960  0
usbhid                 77824  2 hid_logitech_dj,hid_logitech_hidpp
snd_seq_dummy          12288  0
snd_hrtimer            12288  1
snd_seq               110592  7 snd_seq_dummy
snd_seq_device         16384  1 snd_seq
ccm                    20480  6
rfcomm                106496  6
cmac                   12288  2
algif_hash             12288  1
algif_skcipher         12288  1
af_alg                 36864  6 algif_hash,algif_skcipher
nft_queue              12288  3
nft_ct                 28672  17
nft_chain_nat          12288  2
nf_nat                 65536  1 nft_chain_nat
nfnetlink_queue        36864  1
bnep                   36864  2
btusb                  81920  0
btrtl                  36864  1 btusb
btintel                69632  1 btusb
uvcvideo              155648  1
btbcm                  24576  1 btusb
videobuf2_vmalloc      20480  1 uvcvideo
uvc                    12288  1 uvcvideo
btmtk                  32768  1 btusb
videobuf2_memops       16384  1 videobuf2_vmalloc
videobuf2_v4l2         36864  1 uvcvideo
overlay               217088  0
videodev              368640  2 videobuf2_v4l2,uvcvideo
bluetooth            1097728  34 btrtl,btmtk,btintel,btbcm,bnep,btusb,rfcomm
qrtr                   57344  2
videobuf2_common       81920  4 videobuf2_vmalloc,videobuf2_v4l2,uvcvideo,videobuf2_memops
mc                     94208  5 videodev,videobuf2_v4l2,uvcvideo,videobuf2_common
ecdh_generic           16384  1 bluetooth
snd_sof_pci_intel_apl    12288  0
snd_sof_intel_hda_generic    36864  1 snd_sof_pci_intel_apl
soundwire_intel        73728  1 snd_sof_intel_hda_generic
soundwire_generic_allocation    12288  1 soundwire_intel
wl                   6459392  0
soundwire_cadence      45056  1 soundwire_intel
snd_sof_intel_hda_common   184320  2 snd_sof_intel_hda_generic,snd_sof_pci_intel_apl
snd_soc_hdac_hda       28672  1 snd_sof_intel_hda_common
snd_sof_intel_hda_mlink    36864  3 soundwire_intel,snd_sof_intel_hda_common,snd_sof_intel_hda_generic
snd_sof_intel_hda      24576  2 snd_sof_intel_hda_common,snd_sof_intel_hda_generic
snd_sof_pci            24576  2 snd_sof_intel_hda_generic,snd_sof_pci_intel_apl
snd_sof_xtensa_dsp     16384  1 snd_sof_intel_hda_generic
mt7921e                24576  0
snd_sof               397312  4 snd_sof_pci,snd_sof_intel_hda_common,snd_sof_intel_hda_generic,snd_sof_intel_hda
mt7921_common          86016  1 mt7921e
x86_pkg_temp_thermal    16384  0
snd_sof_utils          16384  1 snd_sof
mt792x_lib             69632  2 mt7921e,mt7921_common
snd_soc_acpi_intel_match   126976  2 snd_sof_intel_hda_generic,snd_sof_pci_intel_apl
intel_powerclamp       16384  0
mei_hdcp               28672  0
coretemp               16384  0
mt76_connac_lib        98304  3 mt792x_lib,mt7921e,mt7921_common
kvm_intel             413696  0
snd_soc_acpi           16384  2 snd_soc_acpi_intel_match,snd_sof_intel_hda_generic
intel_rapl_msr         20480  0
soundwire_bus         126976  3 soundwire_intel,soundwire_generic_allocation,soundwire_cadence
at24                   28672  0
mei_pxp                16384  0
ee1004                 16384  0
zram                   49152  1
snd_soc_avs           212992  0
mt76                  139264  4 mt792x_lib,mt7921e,mt7921_common,mt76_connac_lib
spd5118                12288  0
lz4hc_compress         20480  1 zram
lz4_compress           24576  1 zram
kvm                  1396736  1 kvm_intel
snd_soc_hda_codec      24576  1 snd_soc_avs
mac80211             1454080  4 mt792x_lib,mt76,mt7921_common,mt76_connac_lib
xt_hl                  12288  22
snd_hda_codec_hdmi     98304  1
snd_hda_ext_core       36864  6 snd_soc_avs,snd_soc_hda_codec,snd_sof_intel_hda_common,snd_soc_hdac_hda,snd_sof_intel_hda_mlink,snd_sof_intel_hda
ip6t_rt                16384  3
snd_soc_core          421888  6 snd_soc_avs,snd_soc_hda_codec,soundwire_intel,snd_sof,snd_sof_intel_hda_common,snd_soc_hdac_hda
irqbypass              12288  1 kvm
snd_hda_codec_realtek   225280  1
snd_hda_codec_generic   114688  1 snd_hda_codec_realtek
snd_hda_scodec_component    20480  1 snd_hda_codec_realtek
snd_compress           28672  2 snd_soc_avs,snd_soc_core
snd_pcm_dmaengine      16384  1 snd_soc_core
ipt_REJECT             12288  1
processor_thermal_device_pci_legacy    12288  0
nf_reject_ipv4         16384  1 ipt_REJECT
processor_thermal_device    20480  1 processor_thermal_device_pci_legacy
libarc4                12288  1 mac80211
snd_hda_intel          61440  1
xt_LOG                 16384  1
nf_log_syslog          24576  1
processor_thermal_wt_hint    16384  1 processor_thermal_device
snd_intel_dspcfg       40960  5 snd_soc_avs,snd_hda_intel,snd_sof,snd_sof_intel_hda_common,snd_sof_intel_hda_generic
rapl                   20480  0
processor_thermal_rfim    24576  1 processor_thermal_device
cfg80211             1404928  5 wl,mt76,mac80211,mt7921_common,mt76_connac_lib
intel_cstate           20480  0
processor_thermal_rapl    16384  1 processor_thermal_device
nft_limit              16384  2
uinput                 20480  1
snd_intel_sdw_acpi     16384  2 snd_intel_dspcfg,snd_sof_intel_hda_generic
xt_limit               12288  0
mei_me                 57344  2
wmi_bmof               12288  0
intel_rapl_common      53248  2 intel_rapl_msr,processor_thermal_rapl
xt_addrtype            12288  4
snd_hda_codec         217088  8 snd_hda_codec_generic,snd_soc_avs,snd_hda_codec_hdmi,snd_soc_hda_codec,snd_hda_intel,snd_hda_codec_realtek,snd_soc_hdac_hda,snd_sof_intel_hda
xt_tcpudp              16384  60
snd_hda_core          143360  11 snd_hda_codec_generic,snd_soc_avs,snd_hda_codec_hdmi,snd_soc_hda_codec,snd_hda_intel,snd_hda_ext_core,snd_hda_codec,snd_hda_codec_realtek,snd_sof_intel_hda_common,snd_soc_hdac_hda,snd_sof_intel_hda
snd_hwdep              20480  1 snd_hda_codec
snd_pcm               188416  12 snd_soc_avs,snd_hda_codec_hdmi,snd_hda_intel,snd_hda_codec,soundwire_intel,snd_sof,snd_sof_intel_hda_common,snd_compress,snd_soc_core,snd_sof_utils,snd_hda_core,snd_pcm_dmaengine
xt_conntrack           12288  14
nf_conntrack          204800  3 xt_conntrack,nf_nat,nft_ct
nf_defrag_ipv6         24576  1 nf_conntrack
nf_defrag_ipv4         12288  1 nf_conntrack
snd_timer              53248  3 snd_seq,snd_hrtimer,snd_pcm
snd                   151552  17 snd_hda_codec_generic,snd_seq,snd_seq_device,snd_hda_codec_hdmi,snd_hwdep,snd_hda_intel,snd_hda_codec,snd_hda_codec_realtek,snd_sof,snd_timer,snd_compress,snd_soc_core,snd_pcm
nft_compat             20480  105
processor_thermal_wt_req    12288  1 processor_thermal_device
mei                   188416  5 mei_hdcp,mei_pxp,mei_me
soundcore              16384  1 snd
binfmt_misc            28672  1
processor_thermal_power_floor    12288  1 processor_thermal_device
processor_thermal_mbox    12288  4 processor_thermal_power_floor,processor_thermal_wt_req,processor_thermal_rfim,processor_thermal_wt_hint
intel_soc_dts_iosf     16384  1 processor_thermal_device_pci_legacy
ideapad_laptop         49152  0
sparse_keymap          12288  1 ideapad_laptop
platform_profile       12288  1 ideapad_laptop
nls_ascii              12288  1
rfkill                 40960  6 bluetooth,ideapad_laptop,cfg80211
nls_cp437              16384  1
int3403_thermal        16384  0
int340x_thermal_zone    16384  2 int3403_thermal,processor_thermal_device
int3400_thermal        20480  0
vfat                   24576  1
fat                   102400  1 vfat
intel_pmc_core        122880  0
intel_vsec             20480  1 intel_pmc_core
acpi_thermal_rel       20480  1 int3400_thermal
pmt_telemetry          16384  1 intel_pmc_core
pmt_class              16384  1 pmt_telemetry
joydev                 24576  0
ac                     16384  0
evdev                  28672  33
nfsd                 1015808  5
auth_rpcgss           192512  1 nfsd
nfs_acl                12288  1 nfsd
lockd                 163840  1 nfsd
grace                  12288  2 nfsd,lockd
sunrpc                880640  17 nfsd,auth_rpcgss,lockd,nfs_acl
loop                   45056  0
nvme_fabrics           40960  0
nf_tables             372736  702 nft_queue,nft_ct,nft_compat,nft_chain_nat,nft_limit
nvme_keyring           16384  1 nvme_fabrics
parport_pc             40960  0
dm_mod                221184  0
ppdev                  24576  0
nvme_core             225280  1 nvme_fabrics
lp                     20480  0
parport                81920  3 parport_pc,lp,ppdev
efi_pstore             12288  0
nvme_auth              24576  1 nvme_core
configfs               69632  1
nfnetlink              20480  5 nfnetlink_queue,nft_compat,nf_tables
ip_tables              28672  0
x_tables               53248  10 xt_conntrack,nft_compat,xt_LOG,xt_tcpudp,xt_addrtype,ip6t_rt,ipt_REJECT,ip_tables,xt_limit,xt_hl
autofs4                57344  2
ext4                 1146880  2
crc16                  12288  2 bluetooth,ext4
mbcache                16384  1 ext4
jbd2                  200704  1 ext4
btrfs                2174976  0
blake2b_generic        24576  0
raid10                 77824  0
raid456               200704  0
async_raid6_recov      20480  1 raid456
async_memcpy           16384  2 raid456,async_raid6_recov
async_pq               16384  2 raid456,async_raid6_recov
async_xor              16384  3 async_pq,raid456,async_raid6_recov
async_tx               16384  5 async_pq,async_memcpy,async_xor,raid456,async_raid6_recov
xor                    20480  2 async_xor,btrfs
raid6_pq              122880  4 async_pq,btrfs,raid456,async_raid6_recov
libcrc32c              12288  5 nf_conntrack,nf_nat,btrfs,nf_tables,raid456
crc32c_generic         12288  0
raid1                  61440  0
raid0                  28672  0
md_mod                229376  4 raid1,raid10,raid0,raid456
vmd                    24576  0
i915                 4386816  17
drm_buddy              12288  1 i915
i2c_algo_bit           16384  1 i915
drm_display_helper    274432  1 i915
crct10dif_pclmul       12288  1
crc32_pclmul           12288  0
cec                    69632  2 drm_display_helper,i915
crc32c_intel           16384  5
mmc_block              61440  3
hid_multitouch         36864  0
ghash_clmulni_intel    16384  0
rc_core                73728  1 cec
xhci_pci               24576  0
hid_generic            12288  0
sha512_ssse3           53248  1
ahci                   53248  0
libahci                61440  1 ahci
sha256_ssse3           32768  1
libata                471040  2 libahci,ahci
xhci_hcd              364544  1 xhci_pci
sdhci_pci              98304  0
ttm                   102400  1 i915
sha1_ssse3             32768  0
i2c_hid_acpi           12288  0
i2c_hid                45056  1 i2c_hid_acpi
cqhci                  32768  1 sdhci_pci
drm_kms_helper        253952  2 drm_display_helper,i915
hid                   262144  6 i2c_hid,usbhid,hid_multitouch,hid_generic,hid_logitech_dj,hid_logitech_hidpp
rtsx_pci_sdmmc         32768  0
aesni_intel           122880  7
intel_lpss_pci         28672  0
sdhci                  86016  1 sdhci_pci
wdat_wdt               20480  0
scsi_mod              331776  1 libata
drm                   774144  17 i2c_hid,drm_kms_helper,drm_display_helper,drm_buddy,i915,ttm
intel_lpss             12288  1 intel_lpss_pci
gf128mul               16384  1 aesni_intel
crypto_simd            16384  1 aesni_intel
cryptd                 28672  3 crypto_simd,ghash_clmulni_intel
watchdog               49152  1 wdat_wdt
serio_raw              16384  0
i2c_i801               36864  0
usbcore               409600  7 xhci_hcd,usbhid,btmtk,uvcvideo,btusb,xhci_pci,hid_logitech_hidpp
rtsx_pci              147456  1 rtsx_pci_sdmmc
mmc_core              253952  5 rtsx_pci_sdmmc,sdhci,cqhci,mmc_block,sdhci_pci
idma64                 20480  0
video                  81920  2 ideapad_laptop,i915
i2c_smbus              16384  1 i2c_i801
lpc_ich                28672  0
usb_common             16384  3 xhci_hcd,usbcore,uvcvideo
scsi_common            16384  2 scsi_mod,libata
battery                28672  0
wmi                    28672  3 video,wmi_bmof,ideapad_laptop
button                 24576  0
efivarfs               28672  1
rayyan@rayyan:~
$ lsusb
Bus 001 Device 001: ID 1d6b:0002 Linux Foundation 2.0 root hub
Bus 001 Device 002: ID 0489:e0cd Foxconn / Hon Hai MediaTek Bluetooth Adapter
Bus 001 Device 003: ID 04f2:b78e Chicony Electronics Co., Ltd Integrated Camera
Bus 001 Device 005: ID 046d:c542 Logitech, Inc. M185 compact wireless mouse
Bus 001 Device 006: ID 046d:c534 Logitech, Inc. Nano Receiver
Bus 002 Device 001: ID 1d6b:0003 Linux Foundation 3.0 root hub
rayyan@rayyan:~
$ dmesg
dmesg: read kernel buffer failed: Operation not permitted
rayyan@rayyan:~
$ 