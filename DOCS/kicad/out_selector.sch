EESchema Schematic File Version 4
EELAYER 30 0
EELAYER END
$Descr A4 11693 8268
encoding utf-8
Sheet 1 1
Title ""
Date ""
Rev ""
Comp ""
Comment1 ""
Comment2 ""
Comment3 ""
Comment4 ""
$EndDescr
$Comp
L Diode:1N4004 D3
U 1 1 5FB8D7CE
P 2750 5650
F 0 "D3" H 2750 5867 50  0000 C CNN
F 1 "1N4004" H 2750 5776 50  0000 C CNN
F 2 "Diode_SMD:D_1206_3216Metric" H 2750 5475 50  0001 C CNN
F 3 "http://www.vishay.com/docs/88503/1n4001.pdf" H 2750 5650 50  0001 C CNN
	1    2750 5650
	1    0    0    -1  
$EndComp
$Comp
L Transistor_BJT:BC547 Q2
U 1 1 5FB91C65
P 3600 5300
F 0 "Q2" H 3791 5346 50  0000 L CNN
F 1 "BC547" H 3791 5255 50  0000 L CNN
F 2 "Package_TO_SOT_THT:TO-92_Inline" H 3800 5225 50  0001 L CIN
F 3 "https://www.onsemi.com/pub/Collateral/BC550-D.pdf" H 3600 5300 50  0001 L CNN
	1    3600 5300
	0    -1   -1   0   
$EndComp
$Comp
L Device:R R3
U 1 1 5FBA6321
P 3600 5650
F 0 "R3" V 3393 5650 50  0000 C CNN
F 1 "1K" V 3484 5650 50  0000 C CNN
F 2 "Resistor_THT:R_Axial_DIN0207_L6.3mm_D2.5mm_P2.54mm_Vertical" V 3530 5650 50  0001 C CNN
F 3 "~" H 3600 5650 50  0001 C CNN
	1    3600 5650
	-1   0    0    1   
$EndComp
Wire Wire Line
	3100 5200 3150 5200
Wire Wire Line
	2900 5650 3150 5650
Wire Wire Line
	3150 5650 3150 5200
Connection ~ 3150 5200
Wire Wire Line
	2600 5650 2500 5650
Wire Wire Line
	2500 5650 2500 5200
$Comp
L power:GND #PWR015
U 1 1 5FD00B94
P 3800 5200
F 0 "#PWR015" H 3800 4950 50  0001 C CNN
F 1 "GND" H 3805 5027 50  0000 C CNN
F 2 "" H 3800 5200 50  0001 C CNN
F 3 "" H 3800 5200 50  0001 C CNN
	1    3800 5200
	0    -1   -1   0   
$EndComp
$Comp
L Diode:1N4004 D5
U 1 1 600AD959
P 8150 4900
F 0 "D5" H 8150 5117 50  0000 C CNN
F 1 "1N4004" H 8150 5026 50  0000 C CNN
F 2 "Diode_SMD:D_1206_3216Metric" H 8150 4725 50  0001 C CNN
F 3 "http://www.vishay.com/docs/88503/1n4001.pdf" H 8150 4900 50  0001 C CNN
	1    8150 4900
	1    0    0    -1  
$EndComp
$Comp
L Transistor_BJT:BC547 Q3
U 1 1 600AD985
P 9000 4550
F 0 "Q3" H 9191 4596 50  0000 L CNN
F 1 "BC547" H 9191 4505 50  0000 L CNN
F 2 "Package_TO_SOT_THT:TO-92_Inline" H 9200 4475 50  0001 L CIN
F 3 "https://www.onsemi.com/pub/Collateral/BC550-D.pdf" H 9000 4550 50  0001 L CNN
	1    9000 4550
	0    -1   -1   0   
$EndComp
$Comp
L Device:R R4
U 1 1 600AD9B3
P 9000 4900
F 0 "R4" V 8793 4900 50  0000 C CNN
F 1 "1K" V 8884 4900 50  0000 C CNN
F 2 "Resistor_THT:R_Axial_DIN0207_L6.3mm_D2.5mm_P2.54mm_Vertical" V 8930 4900 50  0001 C CNN
F 3 "~" H 9000 4900 50  0001 C CNN
	1    9000 4900
	-1   0    0    1   
$EndComp
Wire Wire Line
	8500 4450 8550 4450
Wire Wire Line
	8300 4900 8550 4900
Wire Wire Line
	8550 4900 8550 4450
Connection ~ 8550 4450
Wire Wire Line
	8550 4450 8700 4450
Wire Wire Line
	8000 4900 7900 4900
Wire Wire Line
	7900 4900 7900 4450
$Comp
L power:GND #PWR020
U 1 1 600ADA0E
P 9200 4450
F 0 "#PWR020" H 9200 4200 50  0001 C CNN
F 1 "GND" H 9205 4277 50  0000 C CNN
F 2 "" H 9200 4450 50  0001 C CNN
F 3 "" H 9200 4450 50  0001 C CNN
	1    9200 4450
	0    -1   -1   0   
$EndComp
$Comp
L Relay:G6S-2 K4
U 1 1 600ADA4C
P 8200 2050
F 0 "K4" V 8967 2050 50  0000 C CNN
F 1 "G6S-2" V 8876 2050 50  0000 C CNN
F 2 "Relay_THT:Relay_DPDT_Omron_G6S-2" H 8200 2050 50  0001 L CNN
F 3 "http://omronfs.omron.com/en_US/ecb/products/pdf/en-g6s.pdf" H 8200 2050 50  0001 C CNN
	1    8200 2050
	0    -1   -1   0   
$EndComp
$Comp
L Relay:G6S-2 K5
U 1 1 600ADAA0
P 8200 4050
F 0 "K5" V 8967 4050 50  0000 C CNN
F 1 "G6S-2" V 8876 4050 50  0000 C CNN
F 2 "Relay_THT:Relay_DPDT_Omron_G6S-2" H 8200 4050 50  0001 L CNN
F 3 "http://omronfs.omron.com/en_US/ecb/products/pdf/en-g6s.pdf" H 8200 4050 50  0001 C CNN
	1    8200 4050
	0    -1   -1   0   
$EndComp
$Comp
L Diode:1N4004 D6
U 1 1 600ADAF4
P 8200 2850
F 0 "D6" H 8200 3067 50  0000 C CNN
F 1 "1N4004" H 8200 2976 50  0000 C CNN
F 2 "Diode_SMD:D_1206_3216Metric" H 8200 2675 50  0001 C CNN
F 3 "http://www.vishay.com/docs/88503/1n4001.pdf" H 8200 2850 50  0001 C CNN
	1    8200 2850
	1    0    0    -1  
$EndComp
Wire Wire Line
	8050 2850 7900 2850
Wire Wire Line
	7900 2850 7900 2450
Wire Wire Line
	8350 2850 8500 2850
Wire Wire Line
	8500 2850 8500 2450
Wire Wire Line
	8700 4450 8700 2850
Wire Wire Line
	8700 2850 8500 2850
Connection ~ 8700 4450
Wire Wire Line
	8700 4450 8800 4450
Connection ~ 8500 2850
$Comp
L Connector:Conn_01x02_Male J12
U 1 1 6016A835
P 8300 5300
F 0 "J12" V 8362 5344 50  0000 L CNN
F 1 "Drive_GPIO" V 8300 5350 50  0000 L CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x02_P2.54mm_Vertical" H 8300 5300 50  0001 C CNN
F 3 "~" H 8300 5300 50  0001 C CNN
	1    8300 5300
	0    1    1    0   
$EndComp
$Comp
L Connector:Conn_01x02_Female J9
U 1 1 600A70A5
P 2900 6050
F 0 "J9" V 2800 5900 50  0000 R CNN
F 1 "PWR_From_Control_Brd" H 3050 5750 50  0000 R CNN
F 2 "TerminalBlock_RND:TerminalBlock_RND_205-00001_1x02_P5.00mm_Horizontal" H 2900 6050 50  0001 C CNN
F 3 "~" H 2900 6050 50  0001 C CNN
	1    2900 6050
	0    -1   -1   0   
$EndComp
$Comp
L Connector:Conn_01x02_Female J11
U 1 1 6010D934
P 4000 6050
F 0 "J11" V 3900 5900 50  0000 R CNN
F 1 "PWR_To_Display_BLK" H 4000 5700 50  0000 R CNN
F 2 "TerminalBlock_RND:TerminalBlock_RND_205-00001_1x02_P5.00mm_Horizontal" H 4000 6050 50  0001 C CNN
F 3 "~" H 4000 6050 50  0001 C CNN
	1    4000 6050
	0    -1   -1   0   
$EndComp
$Comp
L power:GND #PWR014
U 1 1 60111687
P 3000 6250
F 0 "#PWR014" H 3000 6000 50  0001 C CNN
F 1 "GND" H 3005 6077 50  0000 C CNN
F 2 "" H 3000 6250 50  0001 C CNN
F 3 "" H 3000 6250 50  0001 C CNN
	1    3000 6250
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR017
U 1 1 60111D4D
P 4100 6250
F 0 "#PWR017" H 4100 6000 50  0001 C CNN
F 1 "GND" H 4105 6077 50  0000 C CNN
F 2 "" H 4100 6250 50  0001 C CNN
F 3 "" H 4100 6250 50  0001 C CNN
	1    4100 6250
	1    0    0    -1  
$EndComp
$Comp
L rpi_connector-rescue:Conn_01x09_Male-Connector J10
U 1 1 6013FE6D
P 9300 1000
AR Path="/6013FE6D" Ref="J10"  Part="1" 
AR Path="/600F3298/6013FE6D" Ref="J10"  Part="1" 
F 0 "J10" V 9000 750 50  0000 C CNN
F 1 "Audio_Out" V 9150 900 50  0000 C CNN
F 2 "Connector_PinHeader_2.54mm:PinHeader_1x09_P2.54mm_Vertical" H 9300 1000 50  0001 C CNN
F 3 "~" H 9300 1000 50  0001 C CNN
	1    9300 1000
	-1   0    0    1   
$EndComp
Text HLabel 8300 5500 3    50   Input ~ 0
GPIO9
Text HLabel 5400 6000 3    50   Input ~ 0
AUDIO_STREAMING
Text HLabel 2500 4700 0    50   Input ~ 0
FROM_DAC_OUT_R
Text HLabel 3100 4800 2    50   Output ~ 0
TO_HEADAMP_INPUT_R
Text HLabel 7000 3350 0    50   Input ~ 0
AGND_in
Text HLabel 7000 3250 0    50   Output ~ 0
AGND_out_phn
Text HLabel 7000 3150 0    50   Output ~ 0
AGND_out_spk
Text HLabel 7000 3050 0    50   Output ~ 0
AR_out_pnh
Text HLabel 7000 2950 0    50   Input ~ 0
AR_in
Text HLabel 7000 2850 0    50   Output ~ 0
AR_out_spk
Text HLabel 7000 2750 0    50   Output ~ 0
AL_out_phn
Text HLabel 7000 2650 0    50   Input ~ 0
AL_in
Text HLabel 7000 2550 0    50   Output ~ 0
AL_out_spk
Wire Wire Line
	5400 6000 3600 6000
Wire Wire Line
	3600 5800 3600 6000
Wire Wire Line
	8300 5300 8300 5150
Wire Wire Line
	8300 5150 9000 5150
Wire Wire Line
	9000 5150 9000 5050
Wire Wire Line
	7200 2550 7700 2550
Wire Wire Line
	7700 2550 7700 3750
Wire Wire Line
	7700 3750 7900 3750
Wire Wire Line
	7200 2650 7600 2650
Wire Wire Line
	7600 2650 7600 3200
Wire Wire Line
	7600 3200 8500 3200
Wire Wire Line
	8500 3200 8500 3650
Wire Wire Line
	7200 2750 7450 2750
Wire Wire Line
	7450 2750 7450 3450
Wire Wire Line
	7450 3450 7900 3450
Wire Wire Line
	7900 3450 7900 3550
Wire Wire Line
	7200 2850 7400 2850
Wire Wire Line
	7400 2850 7400 4150
Wire Wire Line
	7400 4150 7900 4150
Wire Wire Line
	7200 2950 8550 2950
Wire Wire Line
	8550 2950 8550 3950
Wire Wire Line
	8550 3950 8500 3950
Wire Wire Line
	8500 3950 8500 4050
Wire Wire Line
	7200 3050 7350 3050
Wire Wire Line
	7350 3050 7350 3950
Wire Wire Line
	7350 3950 7900 3950
Wire Wire Line
	7200 3150 7250 3150
Wire Wire Line
	7250 2150 7900 2150
Wire Wire Line
	7200 3250 7550 3250
Wire Wire Line
	7550 3250 7550 1950
Wire Wire Line
	7550 1950 7900 1950
Wire Wire Line
	7200 3350 8800 3350
Wire Wire Line
	8800 3350 8800 2050
Wire Wire Line
	8800 2050 8500 2050
Text HLabel 3100 4400 2    50   Output ~ 0
TO_HEADAMP_INPUT_L
Text HLabel 2500 4300 0    50   Input ~ 0
FROM_DAC_OUT_LEFT
Text HLabel 2500 5650 0    50   Input ~ 0
5V_SWITCH
Text HLabel 2900 6250 0    50   Input ~ 0
5V_SWITCH
Text HLabel 4000 6250 0    50   Input ~ 0
5V_SWITCH
Text HLabel 7900 4900 0    50   Input ~ 0
5V_SWITCH
Text HLabel 7900 2850 0    50   Input ~ 0
5V_SWITCH
Wire Wire Line
	3150 5200 3400 5200
$Comp
L Relay:G6S-2 K3
U 1 1 5FBBDC43
P 2800 4800
F 0 "K3" V 3567 4800 50  0000 C CNN
F 1 "G6S-2" V 3476 4800 50  0000 C CNN
F 2 "Relay_THT:Relay_DPDT_Omron_G6S-2" H 2800 4800 50  0001 L CNN
F 3 "http://omronfs.omron.com/en_US/ecb/products/pdf/en-g6s.pdf" H 2800 4800 50  0001 C CNN
	1    2800 4800
	0    -1   -1   0   
$EndComp
$Comp
L power:GND g
U 1 1 606BE3E1
P 2500 4500
F 0 "g" H 2500 4250 50  0001 C CNN
F 1 "GND" H 2300 4450 50  0000 L CNN
F 2 "" H 2500 4500 50  0001 C CNN
F 3 "" H 2500 4500 50  0001 C CNN
	1    2500 4500
	1    0    0    -1  
$EndComp
$Comp
L power:GND #PWR?
U 1 1 606BED07
P 2500 4900
F 0 "#PWR?" H 2500 4650 50  0001 C CNN
F 1 "GND" H 2505 4727 50  0000 C CNN
F 2 "" H 2500 4900 50  0001 C CNN
F 3 "" H 2500 4900 50  0001 C CNN
	1    2500 4900
	1    0    0    -1  
$EndComp
Wire Wire Line
	7250 3150 7250 2150
$EndSCHEMATC
