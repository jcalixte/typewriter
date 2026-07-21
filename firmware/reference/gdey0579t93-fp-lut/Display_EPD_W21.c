#include "Display_EPD_W21_spi.h"
#include "Display_EPD_W21.h"

unsigned char picData[2888];

void delay_xms(unsigned int xms)
{
	unsigned int i;
	while(xms--)
	{
		i=12000;
		while(i--);
	}
}


void EPD_Display(unsigned char *Image)
{
    unsigned int Width, Height,i,j;
    Width = (EPD_WIDTH % 8 == 0)? (EPD_WIDTH / 8 ): (EPD_WIDTH / 8 + 1);
    Height = EPD_HEIGHT;

    EPD_W21_WriteCMD(0x24);
    for ( j = 0; j < Height; j++) {
        for ( i = 0; i < Width; i++) {
           EPD_W21_WriteDATA(Image[i + j * Width]);
        }
    }
    EPD_Update();		 
}

void Epaper_Spi_WriteByte(unsigned char TxData)
{				   			 
	unsigned char TempData;
	unsigned char scnt;
	TempData=TxData;

  EPD_W21_CLK_0;  
	for(scnt=0;scnt<8;scnt++)
	{ 
		if(TempData&0x80)
		  EPD_W21_MOSI_1 ;
		else
		  EPD_W21_MOSI_0 ;
		EPD_W21_CLK_1;  
	  EPD_W21_CLK_0;  
		TempData=TempData<<1;

  }

}

void Epaper_READBUSY(void)
{ 
  while(1)
  {	 //=1 BUSY
     if(isEPD_W21_BUSY==0) break;;
  }  
}

void Epaper_Write_Command(unsigned char cmd)
{
	EPD_W21_CS_1;
	EPD_W21_CS_0;
	EPD_W21_DC_0;  // D/C#   0:command  1:data

	Epaper_Spi_WriteByte(cmd);
	EPD_W21_CS_1;
}

void Epaper_Write_Data(unsigned char data)
{
	EPD_W21_CS_1;
	EPD_W21_CS_0;
	EPD_W21_DC_1;  // D/C#   0:command  1:data

	Epaper_Spi_WriteByte(data);
	EPD_W21_CS_1;
}


	
/////////////////////////////////////////////////////////////////////
//////////////////////////////////////////////////////////////////////////////////////////////////
/*
u8 LUT_DATA[] = // 全刷
{

//10-50  
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
//LUTW
0x01, 0x4A, 0x0A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x8A, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x8A, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x0A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
//LUTB
0x01, 0x8A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x4A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x4A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x4A, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
       
0x02, 0x00, 0x00,              //FR, XON
0x07, 0x17, 0x41, 0x00, 0x32, 0x00,     //EOPT VGH VSH1 VSH2 VSL VCOM
};
*/

u8 LUT_DATA[] = // 全刷
{
//10-50  
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
//LUTW
0x01, 0x4A, 0x45, 0x00, 0x00, 0x01, 0x00,
0x01, 0x8A, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x8A, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x85, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
//LUTB
0x01, 0x8A, 0x85, 0x00, 0x00, 0x01, 0x00,
0x01, 0x4A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x4A, 0x8A, 0x00, 0x00, 0x01, 0x00,
0x01, 0x45, 0x4A, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
       
0x02, 0x00, 0x00,              //FR, XON
0x27, 0x17, 0x41, 0x00, 0x32, 0x00,     //EOPT VGH VSH1 VSH2 VSL VCOM
};
u8 LUT_DATA1[] = // 快刷
{
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
//LUTW
0x01, 0x14, 0x51, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x01, 0x14, 0x91, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
//LUTB
0x01, 0x94, 0x91, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
0x01, 0x54, 0x51, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 
       
0x02, 0x00, 0x00,              //FR, XON
0x26, 0x17, 0x41, 0xA8, 0x32, 0x00,     //EOPT VGH VSH1 VSH2 VSL VCOM


//fast 1.5s

};
void EPD_HW_Init_LUT(void)
{
	u8 i;
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
	
	Epaper_READBUSY();   
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   
	
    Epaper_Write_Command(0x01); //Driver output control  
    Epaper_Write_Data(0x0F);
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x0e);

    Epaper_Write_Command(0x21);     
    Epaper_Write_Data(0x00);    
    Epaper_Write_Data(0x10);

    Epaper_Write_Command(0x0C);     
    Epaper_Write_Data(0x8B);    
    Epaper_Write_Data(0x9C);    
    Epaper_Write_Data(0xA6);    
    Epaper_Write_Data(0x0F);
    
    Epaper_Write_Command(0x3C); //set border 
    Epaper_Write_Data(0x01);



	    Epaper_Write_Command(0x22);     // 
      Epaper_Write_Data(0x91);    //
	    Epaper_Write_Command(0x20);     // 
    Epaper_READBUSY();	

	    Epaper_Write_Command(0x32);     // 	
	for(i=0;i<227;i++)
	{
	      Epaper_Write_Data(LUT_DATA[i]);
	}
		    Epaper_Write_Command(0x3f);     // 	
	      Epaper_Write_Data(LUT_DATA[227]);
	
      Epaper_Write_Command(0x03);       
      Epaper_Write_Data(LUT_DATA[228]);

      Epaper_Write_Command(0x04);       
      Epaper_Write_Data(LUT_DATA[229]);    
      Epaper_Write_Data(LUT_DATA[230]);    
      Epaper_Write_Data(LUT_DATA[231]);    

      Epaper_Write_Command(0x2C);     // 
      Epaper_Write_Data(LUT_DATA[232]);   
	
	
	    Epaper_Write_Command(0x11);     
    Epaper_Write_Data(0x05); 
  
    Epaper_Write_Command(0x44); //set Ram-X address start/end position   
    Epaper_Write_Data(0x00);
//    Epaper_Write_Data(0x00);    //0x12-->(18+1)*8=152
	  Epaper_Write_Data(0x31);    //0x12-->(18+1)*8=152
  
    Epaper_Write_Command(0x45); //set Ram-Y address start/end position          
//    Epaper_Write_Data(0x97);   //0x97-->(151+1)=152
    Epaper_Write_Data(0x0F);   //0x97-->(151+1)=152  修改的    
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x00);
    Epaper_Write_Data(0x00); 
    
//    Epaper_Write_Command(0x3C); //set border 
//    Epaper_Write_Data(0x01);

    Epaper_Write_Command(0x4E);     
    Epaper_Write_Data(0x00);

	
	  Epaper_Write_Command(0x4F);       
		Epaper_Write_Data(0x0F);
	  Epaper_Write_Data(0x01);	
}
void EPD_HW_Init_LUT_Fast(void)
{
	u8 i;
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
	
	Epaper_READBUSY();   
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   
	
    Epaper_Write_Command(0x01); //Driver output control  
    Epaper_Write_Data(0x0F);
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x0e);

    Epaper_Write_Command(0x21);     
    Epaper_Write_Data(0x00);    
    Epaper_Write_Data(0x10);

    Epaper_Write_Command(0x0C);     
    Epaper_Write_Data(0x8B);    
    Epaper_Write_Data(0x9C);    
    Epaper_Write_Data(0xA6);    
    Epaper_Write_Data(0x0F);
    
    Epaper_Write_Command(0x3C); //set border 
    Epaper_Write_Data(0x01);



	    Epaper_Write_Command(0x22);     // 
      Epaper_Write_Data(0x91);    //
	    Epaper_Write_Command(0x20);     // 
    Epaper_READBUSY();	

	    Epaper_Write_Command(0x32);     // 	
	for(i=0;i<227;i++)
	{
	      Epaper_Write_Data(LUT_DATA1[i]);
	}
		    Epaper_Write_Command(0x3f);     // 	
	      Epaper_Write_Data(LUT_DATA1[227]);
	
      Epaper_Write_Command(0x03);       
      Epaper_Write_Data(LUT_DATA1[228]);

      Epaper_Write_Command(0x04);       
      Epaper_Write_Data(LUT_DATA1[229]);    
      Epaper_Write_Data(LUT_DATA1[230]);    
      Epaper_Write_Data(LUT_DATA1[231]);    

      Epaper_Write_Command(0x2C);     // 
      Epaper_Write_Data(LUT_DATA1[232]);   
	
	
	    Epaper_Write_Command(0x11);     
    Epaper_Write_Data(0x05); 
  
    Epaper_Write_Command(0x44); //set Ram-X address start/end position   
    Epaper_Write_Data(0x00);
//    Epaper_Write_Data(0x00);    //0x12-->(18+1)*8=152
	  Epaper_Write_Data(0x31);    //0x12-->(18+1)*8=152
  
    Epaper_Write_Command(0x45); //set Ram-Y address start/end position          
//    Epaper_Write_Data(0x97);   //0x97-->(151+1)=152
    Epaper_Write_Data(0x0F);   //0x97-->(151+1)=152  修改的    
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x00);
    Epaper_Write_Data(0x00); 
    
//    Epaper_Write_Command(0x3C); //set border 
//    Epaper_Write_Data(0x01);

    Epaper_Write_Command(0x4E);     
    Epaper_Write_Data(0x00);

	
	  Epaper_Write_Command(0x4F);       
		Epaper_Write_Data(0x0F);
	  Epaper_Write_Data(0x01);	
}
//SSD1683
void EPD_HW_Init(void)
{
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
	
	Epaper_READBUSY();   
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   
	
	
}
void EPD_HW_Init_Fast(void)
{
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
  
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   
 	
  Epaper_Write_Command(0x18); //Read built-in temperature sensor
	Epaper_Write_Data(0x80);	
	  	
	Epaper_Write_Command(0x22); // Load temperature value
	Epaper_Write_Data(0xB1);		
  Epaper_Write_Command(0x20);	
  Epaper_READBUSY();   

	Epaper_Write_Command(0x1A); // Write to temperature register
	Epaper_Write_Data(0x64);		
  Epaper_Write_Data(0x00);	
				  	
	Epaper_Write_Command(0x22); // Load temperature value
	Epaper_Write_Data(0x91);		
  Epaper_Write_Command(0x20);	
	Epaper_READBUSY();   
}
void EPD_HW_Init_GUI(void)
{
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
  
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   
 	
  Epaper_Write_Command(0x18); //Read built-in temperature sensor
	Epaper_Write_Data(0x80);	
	  	
	Epaper_Write_Command(0x22); // Load temperature value
	Epaper_Write_Data(0xB1);		
  Epaper_Write_Command(0x20);	
  Epaper_READBUSY();   

	Epaper_Write_Command(0x1A); // Write to temperature register
	Epaper_Write_Data(0x64);		
  Epaper_Write_Data(0x00);	
				  	
	Epaper_Write_Command(0x22); // Load temperature value
	Epaper_Write_Data(0x91);		
  Epaper_Write_Command(0x20);	
	Epaper_READBUSY();   
}
/////////////////////////////////////////////////////////////////////////////////////////
/*When the electronic paper screen is updated, do not unplug the electronic paper to avoid damage to the screen*/
void EPD_Update_LUT(void)
{   
  Epaper_Write_Command(0x22); //Display Update Control
  Epaper_Write_Data(0xC7);   
  Epaper_Write_Command(0x20); //Activate Display Update Sequence
  Epaper_READBUSY();   
	
}
void EPD_Update(void)
{   
  Epaper_Write_Command(0x22); //Display Update Control
  Epaper_Write_Data(0xF7);   
  Epaper_Write_Command(0x20); //Activate Display Update Sequence
  Epaper_READBUSY();   
	
}
void EPD_Update_Fast(void)
{   
  Epaper_Write_Command(0x22); //Display Update Control
  Epaper_Write_Data(0xC7);   
  Epaper_Write_Command(0x20); //Activate Display Update Sequence
  Epaper_READBUSY();   

}
/*When the electronic paper screen is updated, do not unplug the electronic paper to avoid damage to the screen*/
void EPD_Part_Update(void)
{
	Epaper_Write_Command(0x22); //Display Update Control
	Epaper_Write_Data(0xFF);   
	Epaper_Write_Command(0x20); //Activate Display Update Sequence
	Epaper_READBUSY(); 			
}
void EPD_Part_UpdateLUT(void) 
{
	Epaper_Write_Command(0x22); //Display Update Control
	Epaper_Write_Data(0xCF);   
	Epaper_Write_Command(0x20); //Activate Display Update Sequence
	Epaper_READBUSY(); 			
}
void Set_ramMP(void) // Set RAM X - address Start / End position  ; Set RAM Y - address Start / End position  -MASTER
{
	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x05);     // 1 –Y decrement, X increment
	Epaper_Write_Command(0x44);	 						 // Set Ram X- address Start / End position
	Epaper_Write_Data(0x00);     						 // XStart, POR = 00h
	Epaper_Write_Data(0x31); //400/8-1
	Epaper_Write_Command(0x45);	 									// Set Ram Y- address  Start / End position 
	Epaper_Write_Data(0x0f);  
	Epaper_Write_Data(0x01);  //300-1	
	Epaper_Write_Data(0x00);     									// YEnd L
	Epaper_Write_Data(0x00);											// YEnd H 
			
}

void Set_ramMA(void)  // Set RAM X address counter ;  Set RAM Y address counter                             -MASTER
{
	Epaper_Write_Command(0x4e);	 						 
	Epaper_Write_Data(0x00);	
	Epaper_Write_Command(0x4f);	 
	Epaper_Write_Data(0x0f);  
	Epaper_Write_Data(0x01); 	
}

void Set_ramSP(void)  // Set RAM X - address Start / End position  ; Set RAM Y - address Start / End position  -SLAVE
{
	Epaper_Write_Command(0x91);	 						 
	Epaper_Write_Data(0x04); 
	Epaper_Write_Command(0xc4);	 						 // Set Ram X- address Start / End position
	Epaper_Write_Data(0x30);   //392/8-1   						 // XStart, POR = 00h
	Epaper_Write_Data(0x00); 
	Epaper_Write_Command(0xc5);	 									// Set Ram Y- address  Start / End position 
	Epaper_Write_Data(0x0f);  
	Epaper_Write_Data(0x01);  	
	Epaper_Write_Data(0x00);     									// YEnd L
	Epaper_Write_Data(0x00);											// YEnd H 	
}

void Set_ramSA(void)   // Set RAM X address counter ;  Set RAM Y address counter                             -SLAVE
{
	Epaper_Write_Command(0xce);	 						 
	Epaper_Write_Data(0x31); 
	Epaper_Write_Command(0xcf);	 
	Epaper_Write_Data(0x0f);  
	Epaper_Write_Data(0x01); 	
}

//////////////////////////////All screen update////////////////////////////////////////////

//Horizontal scanning, from right to left, from bottom to top
void EPD_WhiteScreen_ALL_Fast(const unsigned char *datas)
{
  u32 i; 
  u8 tempOriginal;      
  u32 tempcol=0;
  u32 templine=0;

    Epaper_Write_Command(0x11);     
    Epaper_Write_Data(0x05); 
  
    Epaper_Write_Command(0x44); //set Ram-X address start/end position   
    Epaper_Write_Data(0x00);
	  Epaper_Write_Data(0x31);    //0x12-->(18+1)*8=152
  
    Epaper_Write_Command(0x45); //set Ram-Y address start/end position          
    Epaper_Write_Data(0x0F);   //0x97-->(151+1)=152      
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x00);
    Epaper_Write_Data(0x00); 

    Epaper_Write_Command(0x4E);     
    Epaper_Write_Data(0x00);	
	  Epaper_Write_Command(0x4F);       
		Epaper_Write_Data(0x0F);
	  Epaper_Write_Data(0x01);	
	
	Epaper_READBUSY();
    Epaper_Write_Command(0x24);   //write RAM for black(0)/white (1)	
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
   {          
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
   } 
	 
    Epaper_Write_Command(0x26);   //write RAM for black(0)/white (1)	
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
   {            
     Epaper_Write_Data(0X00);
   } 	 
	 

    Epaper_Write_Command(0x91);     
    Epaper_Write_Data(0x04); 
  
    Epaper_Write_Command(0xC4); //set Ram-X address start/end position   
    Epaper_Write_Data(0x31);
	  Epaper_Write_Data(0x00);    //0x12-->(18+1)*8=152
  
    Epaper_Write_Command(0xC5); //set Ram-Y address start/end position          
    Epaper_Write_Data(0x0F);   //0x97-->(151+1)=152  修改的    
    Epaper_Write_Data(0x01);
    Epaper_Write_Data(0x00);
    Epaper_Write_Data(0x00); 
    
    Epaper_Write_Command(0xCE);     
    Epaper_Write_Data(0x31);	
	  Epaper_Write_Command(0xCF);       
		Epaper_Write_Data(0x0F);
	  Epaper_Write_Data(0x01);	
	
	Epaper_READBUSY();

	tempcol=tempcol-1; //Byte dislocation processing
	templine=0;
    Epaper_Write_Command(0xa4);   //write RAM for black(0)/white (1)
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
   {          
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
   } 
	 
    Epaper_Write_Command(0xa6);   //write RAM for black(0)/white (1)	
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
   {            
     Epaper_Write_Data(0X00);
   } 	
	 
   EPD_Update_Fast();	 
}
///////////////////////////Part update//////////////////////////////////////////////
//The x axis is reduced by one byte, and the y axis is reduced by one pixel.
void EPD_SetRAMValue_BaseMap( const unsigned char * datas)
{
	u32 i; 
	  u8 tempOriginal;      
  u32 tempcol=0;
  u32 templine=0;
Set_ramMP();
Set_ramMA();
	Epaper_Write_Command(0x24);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}

Set_ramMA();
	Epaper_Write_Command(0x26);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}

tempcol=tempcol-1; //Byte dislocation processing
templine=0;
Set_ramSP();
Set_ramSA();
	Epaper_Write_Command(0xA4);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal); 
    // Epaper_Write_Data(0xff);		 
	}

Set_ramSA();
	Epaper_Write_Command(0xA6);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}	
	EPD_Update();
	 //EPD_Update_LUT();
	
	
	//Reset
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
	 Epaper_READBUSY();
	
	 //basemap  
Set_ramMA();
	tempcol=0;
	templine=0;
	Epaper_Write_Command(0x26);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}
Set_ramSA();
	tempcol=tempcol-1; //Byte dislocation processing
	templine=0;
	Epaper_Write_Command(0xa6);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}	
}

void EPD_SetRAMValue_BaseMap_LUT( const unsigned char * datas)
{
	u32 i; 
	  u8 tempOriginal;      
  u32 tempcol=0;
  u32 templine=0;
Set_ramMP();
Set_ramMA();
	Epaper_Write_Command(0x24);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}

Set_ramMA();
	Epaper_Write_Command(0x26);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}

tempcol=tempcol-1; //Byte dislocation processing
templine=0;
Set_ramSP();
Set_ramSA();
	Epaper_Write_Command(0xA4);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal); 
    // Epaper_Write_Data(0xff);		 
	}

Set_ramSA();
	Epaper_Write_Command(0xA6);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}	
	//EPD_Update();
	 EPD_Update_LUT();
	
	
	//Reset
	EPD_W21_RST_0;  // Module reset   
	delay_xms(10);//At least 10ms delay 
	EPD_W21_RST_1;
	delay_xms(10); //At least 10ms delay 
	 Epaper_READBUSY();
	
	 //basemap  
Set_ramMA();
	tempcol=0;
	templine=0;
	Epaper_Write_Command(0x26);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}
Set_ramSA();
	tempcol=tempcol-1; //Byte dislocation processing
	templine=0;
	Epaper_Write_Command(0xa6);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
     tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
     templine++;
     if(templine>=Gate_BITS)
     {
       tempcol++;
       templine=0;
     }     
     Epaper_Write_Data(~tempOriginal);
		 //Epaper_Write_Data(0xff);
	}	
}

void EPD_DeepSleep(void)
{  	
  Epaper_Write_Command(0x10); //enter deep sleep
  Epaper_Write_Data(0x01); 
  delay_xms(100);
}



/////////////////////////////////Single display////////////////////////////////////////////////

void EPD_WhiteScreen_White(void)

{
	u32 i; 
	
Set_ramMP();
Set_ramMA();
	Epaper_Write_Command(0x24);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0xff);
	}

Set_ramMA();
	Epaper_Write_Command(0x26);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}

		
Set_ramSP();
Set_ramSA();
	Epaper_Write_Command(0xA4);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0xff);
	}

Set_ramSA();
	Epaper_Write_Command(0xA6);   
	for(i=0;i<Source_BYTES*Gate_BITS;i++)
	{
			Epaper_Write_Data(0x00);
	}	
		//EPD_Update();
	 EPD_Update_LUT();
	

}

void EPD_WhiteScreen_ALL(const unsigned char *datas)

{
		u32 i; 
		u8 tempOriginal;      
		u32 tempcol=0;
		u32 templine=0;
		Set_ramMP();
		Set_ramMA();
		Epaper_Write_Command(0x24);   
		for(i=0;i<Source_BYTES*Gate_BITS;i++)
		{
			tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
			templine++;
			if(templine>=Gate_BITS)
			{
			tempcol++;
			templine=0;
			}     
			Epaper_Write_Data(tempOriginal);
		}

		Set_ramMA();
		Epaper_Write_Command(0x26);   
		for(i=0;i<Source_BYTES*Gate_BITS;i++)
		{
		  Epaper_Write_Data(0x00);
		}

		tempcol=tempcol-1; //Byte dislocation processing
		templine=0;
		Set_ramSP();
		Set_ramSA();
		Epaper_Write_Command(0xA4);   
		for(i=0;i<Source_BYTES*Gate_BITS;i++)
		{
			tempOriginal=*(datas+templine*Source_BYTES*2+tempcol);
			templine++;
			if(templine>=Gate_BITS)
			{
			tempcol++;
			templine=0;
			}     
			Epaper_Write_Data(tempOriginal); 	 
		}

		Set_ramSA();
		Epaper_Write_Command(0xA6);   
		for(i=0;i<Source_BYTES*Gate_BITS;i++)
		{
		  Epaper_Write_Data(0x00);
		}	
		//EPD_Update();
	 EPD_Update_LUT();

}
/*
void EPD_HW_Init_Part(void)
{

	Epaper_READBUSY();   
	Epaper_Write_Command(0x12);  //SWRESET
	Epaper_READBUSY();   

	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x03);     // 1 –Y decrement, X increment
		
	Epaper_Write_Command(0x3C); //BorderWavefrom
	Epaper_Write_Data(0x80);		
	
}
*/

//Horizontal scanning
void EPD_Dis_Part_M(unsigned int x_start,unsigned int y_start,const unsigned char * datas,unsigned int PART_COLUMN,unsigned int PART_LINE)
{
	unsigned int i,j;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;
	
  char tempData,data1;

	
	
	x_start=x_start/8;
	x_end=x_start+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_start;
	if(y_start>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256; 
	}
	y_end1=0;
	y_end2=y_start+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	
	
//	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_start);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    // ????=0	


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_start); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing
				 tempData=datas[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }

	 EPD_Part_Update();

}
void EPD_Dis_Part_S(unsigned int x_start,unsigned int y_start,const unsigned char * datas,unsigned int PART_COLUMN,unsigned int PART_LINE)
{
	unsigned int i;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;
	x_start=x_start/8;
	x_end=x_start+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_start;
	if(y_start>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256; 
	}
	y_end1=0;
	y_end2=y_start+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		

	
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	
	
//
	Epaper_Write_Command(0x91);    						 
	Epaper_Write_Data(0x03); 
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_start);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    // ????=0	


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_start); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
	 
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datas[i]);
   }

	 EPD_Part_Update();

}

/////////////////////////////////////TIME///////////////////////////////////////////////////
void EPD_Dis_Part_myself_M(unsigned int x_startA,unsigned int y_startA,const unsigned char * datasA,
	                       unsigned int x_startB,unsigned int y_startB,const unsigned char * datasB,
												 unsigned int x_startC,unsigned int y_startC,const unsigned char * datasC,
												 unsigned int x_startD,unsigned int y_startD,const unsigned char * datasD,
											   unsigned int x_startE,unsigned int y_startE,const unsigned char * datasE,
												 unsigned int PART_COLUMN,unsigned int PART_LINE
	                      )
{
	unsigned int i,j;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;
	
  char tempData,data1;

	
	//Data A//////////////////////////////
	x_startA=x_startA/8;//Convert to byte
	x_end=x_startA+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startA-1;
	if(y_startA>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startA+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		

//
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	
	
//	
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startA);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startA); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing	 
				 tempData=datasA[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }
	 
	//Data B/////////////////////////////////////

	x_startB=x_startB/8;//Convert to byte
	x_end=x_startB+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startB-1;
	if(y_startB>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startB+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startB);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startB); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing 
				 tempData=datasB[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }
	 
	//Data C//////////////////////////////////////
	x_startC=x_startC/8;//Convert to byte
	x_end=x_startC+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startC-1;
	if(y_startC>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startC+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startC);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startC); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing 
				 tempData=datasC[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }	 	 
 	 
	//Data D//////////////////////////////////////
	x_startD=x_startD/8;//Convert to byte
	x_end=x_startD+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startD-1;
	if(y_startD>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startD+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startD);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);        // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startD); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing
				 tempData=datasD[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }
	//Data E//////////////////////////////////////
	x_startE=x_startE/8;//Convert to byte
	x_end=x_startE+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startE-1;
	if(y_startE>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startE+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startE);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startE); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing		 
				 tempData=datasE[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }  
	 EPD_Part_Update(); 

}

void EPD_Dis_Part_myself_S(unsigned int x_startA,unsigned int y_startA,const unsigned char * datasA,
	                       unsigned int x_startB,unsigned int y_startB,const unsigned char * datasB,
												 unsigned int x_startC,unsigned int y_startC,const unsigned char * datasC,
												 unsigned int x_startD,unsigned int y_startD,const unsigned char * datasD,
											   unsigned int x_startE,unsigned int y_startE,const unsigned char * datasE,
												 unsigned int PART_COLUMN,unsigned int PART_LINE
	                      )
{
	unsigned int i;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;


	//Data A//////////////////////////////
	x_startA=x_startA/8;//Convert to byte
	x_end=x_startA+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startA-1;
	if(y_startA>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startA+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		

//
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	

	Epaper_Write_Command(0x91);    						 
	Epaper_Write_Data(0x03); 	
//	
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startA);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startA); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasA[i]);
   }

	 
	//Data B/////////////////////////////////////

	x_startB=x_startB/8;//Convert to byte
	x_end=x_startB+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startB-1;
	if(y_startB>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startB+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startB);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startB); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasB[i]);
   }

	 
	//Data C//////////////////////////////////////
	x_startC=x_startC/8;//Convert to byte
	x_end=x_startC+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startC-1;
	if(y_startC>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startC+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startC);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startC); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasC[i]);
   }
 	 
 	 
	//Data D//////////////////////////////////////
	x_startD=x_startD/8;//Convert to byte
	x_end=x_startD+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startD-1;
	if(y_startD>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startD+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startD);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);        // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startD); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasD[i]);
   }

	//Data E//////////////////////////////////////
	x_startE=x_startE/8;//Convert to byte
	x_end=x_startE+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startE-1;
	if(y_startE>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startE+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startE);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startE); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasE[i]);
   }
 
	 //EPD_Part_Update(); 
	 EPD_Part_UpdateLUT(); 

}
void EPD_Dis_Part_myself_S_LUT(unsigned int x_startA,unsigned int y_startA,const unsigned char * datasA,
	                       unsigned int x_startB,unsigned int y_startB,const unsigned char * datasB,
												 unsigned int x_startC,unsigned int y_startC,const unsigned char * datasC,
												 unsigned int x_startD,unsigned int y_startD,const unsigned char * datasD,
											   unsigned int x_startE,unsigned int y_startE,const unsigned char * datasE,
												 unsigned int PART_COLUMN,unsigned int PART_LINE
	                      )
{
	unsigned int i;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;


	//Data A//////////////////////////////
	x_startA=x_startA/8;//Convert to byte
	x_end=x_startA+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startA-1;
	if(y_startA>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startA+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		

//
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	

	Epaper_Write_Command(0x91);    						 
	Epaper_Write_Data(0x03); 	
//	
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startA);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startA); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasA[i]);
   }

	 
	//Data B/////////////////////////////////////

	x_startB=x_startB/8;//Convert to byte
	x_end=x_startB+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startB-1;
	if(y_startB>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startB+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startB);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startB); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasB[i]);
   }

	 
	//Data C//////////////////////////////////////
	x_startC=x_startC/8;//Convert to byte
	x_end=x_startC+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startC-1;
	if(y_startC>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startC+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startC);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startC); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasC[i]);
   }
 	 
 	 
	//Data D//////////////////////////////////////
	x_startD=x_startD/8;//Convert to byte
	x_end=x_startD+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startD-1;
	if(y_startD>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startD+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startD);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);        // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startD); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasD[i]);
   }

	//Data E//////////////////////////////////////
	x_startE=x_startE/8;//Convert to byte
	x_end=x_startE+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startE-1;
	if(y_startE>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startE+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startE);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startE); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasE[i]);
   }
 
	 //EPD_Part_Update(); 
	 EPD_Part_UpdateLUT(); 

}
void EPD_Dis_Part_myself_All(unsigned int x_startA,unsigned int y_startA,const unsigned char * datasA,
	                       unsigned int x_startB,unsigned int y_startB,const unsigned char * datasB,
												 unsigned int x_startC,unsigned int y_startC,const unsigned char * datasC,
												 unsigned int x_startD,unsigned int y_startD,const unsigned char * datasD,
												 unsigned int x_startE,unsigned int y_startE,const unsigned char * datasE,
												 unsigned int PART_COLUMN,unsigned int PART_LINE
	                      )
{
	unsigned int i,j;  
	unsigned int x_end,y_start1,y_start2,y_end1,y_end2;
  char tempData,data1;

	
	
//
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 	

	Epaper_Write_Command(0x91);    						 
	Epaper_Write_Data(0x03); 	
//	
	
	//Data A//////////////////////////////
	x_startA=x_startA/8;//Convert to byte
	x_end=x_startA+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startA-1;
	if(y_startA>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startA+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startA);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startA); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasA[i]);
   }  

	 
	//Data B/////////////////////////////////////

	x_startB=x_startB/8;//Convert to byte
	x_end=x_startB+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startB-1;
	if(y_startB>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startB+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startB);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startB); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasB[i]);
   }
	 
	//Data C/////////////////////////////////////

	x_startC=x_startC/8;//Convert to byte
	x_end=x_startC+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startC-1;
	if(y_startC>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startC+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0xC4);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startC);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0xC5);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0xCE);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startC); 
	Epaper_Write_Command(0xCF);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0xA4);   //Write Black and White image to RAM
   for(i=0;i<PART_COLUMN*PART_LINE/8;i++)
   {   
     Epaper_Write_Data(~datasC[i]);
   }	
	 
	/***************************************************************************/ 
//
  Epaper_Write_Command(0x22); 
  Epaper_Write_Data(0xc0);   
  Epaper_Write_Command(0x20); 
  Epaper_READBUSY(); 		
//
			 
//Data D//////////////////////////////////////
	x_startD=x_startD/8;//Convert to byte
	x_end=x_startD+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startD-1;
	if(y_startD>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startD+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startD);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);    // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);   


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startD); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM
	 for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing	 
				 tempData=datasD[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }
 	 
	//Data E//////////////////////////////////////
	x_startE=x_startE/8;//Convert to byte
	x_end=x_startE+PART_LINE/8-1; 
	
	y_start1=0;
	y_start2=y_startE-1;
	if(y_startE>=256)
	{
		y_start1=y_start2/256;
		y_start2=y_start2%256;
	}
	y_end1=0;
	y_end2=y_startE+PART_COLUMN-1;
	if(y_end2>=256)
	{
		y_end1=y_end2/256;
		y_end2=y_end2%256;		
	}		
	
	Epaper_Write_Command(0x44);       // set RAM x address start/end, in page 35
	Epaper_Write_Data(x_startE);    // RAM x address start at 00h;
	Epaper_Write_Data(x_end);        // RAM x address end at 0fh(15+1)*8->128 
	Epaper_Write_Command(0x45);       // set RAM y address start/end, in page 35
	Epaper_Write_Data(y_start2);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_start1);    // RAM y address start at 0127h;
	Epaper_Write_Data(y_end2);    // RAM y address end at 00h;
	Epaper_Write_Data(y_end1);    


	Epaper_Write_Command(0x4E);   // set RAM x address count to 0;
	Epaper_Write_Data(x_startE); 
	Epaper_Write_Command(0x4F);   // set RAM y address count to 0X127;    
	Epaper_Write_Data(y_start2);
	Epaper_Write_Data(y_start1);
	
	
	 Epaper_Write_Command(0x24);   //Write Black and White image to RAM

   for(i=0;i<PART_COLUMN;i++)
	    for(j=0;j<PART_LINE/8;j++)
			 {   
				 //Byte image processing 
				 tempData=datasE[i*(PART_LINE/8)+(PART_LINE/8)-j-1];	 
				 data1=(tempData>>7&0x01)+(tempData>>5&0x02)+(tempData>>3&0x04)+(tempData>>1&0x08)+(tempData<<7&0x80)+(tempData<<5&0x40)+(tempData<<3&0x20)+(tempData<<1&0x10); //字节内数据反序
		     Epaper_Write_Data(~data1); 
				 
			 }
	 EPD_Part_Update(); 

}




void EPD_Part_init_LUT11(void)
{

    Epaper_Write_Command(0x21);     
    Epaper_Write_Data(0x00);    
    Epaper_Write_Data(0x10);

    Epaper_Write_Command(0x0C);     
    Epaper_Write_Data(0x8B);    
    Epaper_Write_Data(0x9C);    
    Epaper_Write_Data(0xA6);    
    Epaper_Write_Data(0x0F);
    
	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x03);     // 1 ¨CY decrement, X increment
		
	Epaper_Write_Command(0x3C); //BorderWavefrom
	Epaper_Write_Data(0x80);	
	}


//解决局刷掉色问题
u8 LUT_DATA_part[] = //5.79
{
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
 
0x01, 0x18, 0x01, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
 
0x01, 0x58, 0x41, 0x00, 0x00, 0x01, 0x00,
0x01, 0x41, 0x00, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,  
 
0x01, 0x98, 0x81, 0x00, 0x00, 0x01, 0x00,
0x01, 0x81, 0x00, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,

0x01, 0x18, 0x41, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x01, 0x01, 0x00, 0x00, 0x00, 0x01, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,

0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 


0x04, 0x00, 0x00,    //FR, XON
0x06, 0x17, 0x41, 0xA8, 0x32, 0x00, 
//EOPT  VGH   VSH1  VSH2  VSL   VCOM  
};

void EPD_Part_init_LUT(void)
{
		u8 i;

    Epaper_Write_Command(0x01); //Driver output control      
//     Epaper_Write_Data(0x97);
     Epaper_Write_Data(0x0F);
//     Epaper_Write_Data(0x00);
     Epaper_Write_Data(0x01);
     Epaper_Write_Data(0x0E);
	
	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x03);     // 1 –Y decrement, X increment
		
    
    Epaper_Write_Command(0x3C); //set border 
    Epaper_Write_Data(0xc0);
	
	    Epaper_Write_Command(0x32);     // 	
	for(i=0;i<227;i++)
	{
	      Epaper_Write_Data(LUT_DATA_part[i]);
	}
		    Epaper_Write_Command(0x3f);     // 	
	      Epaper_Write_Data(LUT_DATA_part[227]);
  
		    Epaper_Write_Command(0xBf);     // 	
	      Epaper_Write_Data(0X22);
	
      Epaper_Write_Command(0x03);       
      Epaper_Write_Data(LUT_DATA_part[228]);

      Epaper_Write_Command(0x04);       
      Epaper_Write_Data(LUT_DATA_part[229]);    
      Epaper_Write_Data(LUT_DATA_part[230]);    
      Epaper_Write_Data(LUT_DATA_part[231]);    

      Epaper_Write_Command(0x2C);     // 
      Epaper_Write_Data(LUT_DATA_part[232]);   
	
  

//      Epaper_Write_Command(0x04);       
//      Epaper_Write_Data(0x41);    
//      Epaper_Write_Data(0xA8);    
//      Epaper_Write_Data(0x32);    

//      Epaper_Write_Command(0x2C);     // 
//      Epaper_Write_Data(0x00); 
      
//	Epaper_Write_Command(0x22);
//	Epaper_Write_Data(0xc0);			
//	Epaper_Write_Command(0x20);
//	Epaper_READBUSY();	
  
  Epaper_Write_Command(0x37); 
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00); 
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x40);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);    
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00); 
      
//	Epaper_Write_Command(0x22);
//	Epaper_Write_Data(0xc0);			
//	Epaper_Write_Command(0x20);
//	Epaper_READBUSY();	

//	Epaper_Write_Command(0x3C);       
// 	Epaper_Write_Data(0x80);
  
//  Epaper_Write_Command(0x3f);     // 	
//  Epaper_Write_Data(LUT_DATA[153]);

//  Epaper_Write_Command(0x21); 
//  Epaper_Write_Data(0x00);  
//  Epaper_Write_Data(0x00);

//  Epaper_Write_Command(0x22); 
//  Epaper_Write_Data(0xc0);   
//  Epaper_Write_Command(0x20); 
//  Epaper_READBUSY(); 
}	













void EPD_HW_Init_Part(void)
{
//    delay_ms(100); 
//    Epaper_RESET=0;     
//    delay_ms(10); 
//    Epaper_RESET=1; //hard reset  
//    delay_ms(10);  
//    Epaper_READBUSY();
	
//	Epaper_READBUSY();   
//	Epaper_Write_Command(0x12);  //SWRESET
//	Epaper_READBUSY();   


    Epaper_Write_Command(0x21);     
    Epaper_Write_Data(0x00);    
    Epaper_Write_Data(0x10);

    Epaper_Write_Command(0x0C);     
    Epaper_Write_Data(0x8B);    
    Epaper_Write_Data(0x9C);    
    Epaper_Write_Data(0xA6);    
    Epaper_Write_Data(0x0F);
    
	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x03);     // 1 ¨CY decrement, X increment
		
	Epaper_Write_Command(0x3C); //BorderWavefrom
	Epaper_Write_Data(0x80);		
	
}


void Epaper_Init1(void)
{
    Epaper_Write_Command(0x01); //Driver output control      
//     Epaper_Write_Data(0x97);
     Epaper_Write_Data(0x0F);
//     Epaper_Write_Data(0x00);
     Epaper_Write_Data(0x01);
     Epaper_Write_Data(0x0E);
	
	Epaper_Write_Command(0x11);	 // Data Entry mode setting
	Epaper_Write_Data(0x03);     // 1 –Y decrement, X increment
	

//    Epaper_Write_Command(0x44); //set Ram-X address start/end position   
//    Epaper_Write_Data(0x01);
////    Epaper_Write_Data(0x00);    //0x12-->(18+1)*8=152
//	Epaper_Write_Data(0x13);    //0x12-->(18+1)*8=152
//    Epaper_Write_Command(0x45); //set Ram-Y address start/end position          
////    Epaper_Write_Data(0x97);   //0x97-->(151+1)=152
//    Epaper_Write_Data(0x97);   //0x97-->(151+1)=152  修改的    
//    Epaper_Write_Data(0x00);
//    Epaper_Write_Data(0x00);
//    Epaper_Write_Data(0x00); 
    
    Epaper_Write_Command(0x3C); //set border 
    Epaper_Write_Data(0xc0);

//  Epaper_Write_Command(0x4E);     
//    Epaper_Write_Data(0x01);

//	
//	  Epaper_Write_Command(0x4F);       
//		Epaper_Write_Data(0x97);
//	  Epaper_Write_Data(0x00);
    
}	

void Epaper_Partial(void)
{
//      delay_ms(100); 
//    Epaper_RESET=0;     
//    delay_ms(10); 
//    Epaper_RESET=1; //hard reset  
//    delay_ms(10);  
    	u8 i;


//	    Epaper_Write_Command(0x22);     // 
//      Epaper_Write_Data(0x91);    //
//	    Epaper_Write_Command(0x20);     // 
//    Epaper_READBUSY();	
//  
	    Epaper_Write_Command(0x32);     // 	
	for(i=0;i<227;i++)
	{
	      Epaper_Write_Data(LUT_DATA_part[i]);
	}
		    Epaper_Write_Command(0x3f);     // 	
	      Epaper_Write_Data(LUT_DATA_part[227]);
  
//		    Epaper_Write_Command(0xBf);     // 	
//	      Epaper_Write_Data(0X22);
	
      Epaper_Write_Command(0x03);       
      Epaper_Write_Data(LUT_DATA_part[228]);

      Epaper_Write_Command(0x04);       
      Epaper_Write_Data(LUT_DATA_part[229]);    
      Epaper_Write_Data(LUT_DATA_part[230]);    
      Epaper_Write_Data(LUT_DATA_part[231]);    

      Epaper_Write_Command(0x2C);     // 
      Epaper_Write_Data(LUT_DATA_part[232]);   
	
  

//      Epaper_Write_Command(0x04);       
//      Epaper_Write_Data(0x41);    
//      Epaper_Write_Data(0xA8);    
//      Epaper_Write_Data(0x32);    

//      Epaper_Write_Command(0x2C);     // 
//      Epaper_Write_Data(0x00); 
      
//	Epaper_Write_Command(0x22);
//	Epaper_Write_Data(0xc0);			
//	Epaper_Write_Command(0x20);
//	Epaper_READBUSY();	
  
  Epaper_Write_Command(0x37); 
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00); 
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x40);  
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00);    
  Epaper_Write_Data(0x00);  
  Epaper_Write_Data(0x00); 
      
//	Epaper_Write_Command(0x22);
//	Epaper_Write_Data(0xc0);			
//	Epaper_Write_Command(0x20);
//	Epaper_READBUSY();	

//	Epaper_Write_Command(0x3C);       
// 	Epaper_Write_Data(0x80);
  
//  Epaper_Write_Command(0x3f);     // 	
//  Epaper_Write_Data(LUT_DATA[153]);

//  Epaper_Write_Command(0x21); 
//  Epaper_Write_Data(0x00);  
//  Epaper_Write_Data(0x00);

//  Epaper_Write_Command(0x22); 
//  Epaper_Write_Data(0xc0);   
//  Epaper_Write_Command(0x20); 
//  Epaper_READBUSY(); 

}

/***********************************************************
						end file
***********************************************************/

